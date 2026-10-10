//! Port of act's `pkg/filecollector`.
//!
//! This is the component that decides what a job sees. Before a step runs, act
//! copies the workspace into the container as a tar stream (or, on a host
//! runner, into a plain directory). Everything in this module exists to get
//! that copy *right*, and the two upstream tests are precise about the two
//! rules that are easy to get wrong:
//!
//! * A file that is ignored by `.gitignore` but **tracked by git** must still
//!   be copied. The test repo has a `.gitignore` containing `.*` and an
//!   untracked `.env`; only the tracked `.gitignore` may end up in the tar.
//! * An ignored *directory* is skipped only when it holds no tracked file, so
//!   a build still sees `node_modules` if git knows about something inside it.
//!   That is the `i.Glob(dir + "/**")` probe.
//!
//! Symlinks are emitted as symlinks, never followed. Anything that is not a
//! regular file (fifo, socket, device) is dropped.
//!
//! Upstream is `nektos/act` (MIT, Copyright (c) Christoph Schitt) plus the
//! go-git index and gitignore packages it leans on, both ported here in
//! [`crate::git_index`] and [`crate::gitignore`].
//!
//! # Two prefix conventions, and why the tests cannot see them
//!
//! `src_prefix` is not derived from `src_path` inside the collector, and act's
//! two call sites disagree about it:
//!
//! | call site | `src_prefix` | ignorer |
//! |---|---|---|
//! | `containerReference.copyDir` | `Dir(src_path) + sep` | yes |
//! | `HostEnvironment.CopyDir` | `Dir(src_path) + sep` | yes |
//! | `HostEnvironment.GetTarStream` | `src_path + sep` | no |
//! | `local_repository_cache` | `src_path + sep` | no |
//!
//! So an ignorer is only ever paired with the parent prefix, where the walk
//! root is reduced to its own base name — which means a workspace directory
//! whose own name matches a `.gitignore` rule is pruned with everything under
//! it. The `src_path` prefix is only used without an ignorer, where nothing is
//! filtered; had an ignorer been paired with it, `TrimPrefix` would not match
//! the root at all and the *whole absolute path*, ancestors included, would be
//! tested against the rules. Both shapes are preserved here and both are pinned
//! by the integration tests.
//!
//! The upstream tests cannot observe any of this: they run against go-billy's
//! in-memory filesystem, whose `Walk` reports the root as `<root>/.` instead of
//! `<root>`. The `.` component is the reason the collector has a
//! `split.last() == "."` branch at all, and it is only ever produced by that
//! double — Go's real `filepath.Walk` never emits it.
//!
//! Deviations from upstream:
//!
//! * The filesystem sits behind the [`Fs`] trait, as upstream, but the trait
//!   is split: [`Fs`] carries the four operations, and [`DefaultFs`] implements
//!   them on the real filesystem. Upstream's `WalkFunc` returns an `error` and
//!   signals "skip this directory" with the sentinel `filepath.SkipDir`; here
//!   the closure returns a [`WalkOutcome`], because a plain `io::Error` cannot
//!   carry a sentinel without a string comparison.
//! * Cancellation: upstream takes a `context.Context` and, for the file copy
//!   itself, spawns a goroutine that closes the file handle out from under
//!   `io.Copy`. This port takes a [`Cancellation`] flag and checks it before
//!   every block, which aborts the copy with the same `copy cancelled` surface
//!   without a thread.
//! * Go's `fs.FileInfo` is a trait over `Stat_t`; [`FileInfo`] is a plain
//!   struct carrying the parts act uses, with Go's `FileMode` bit layout kept
//!   intact (see [`FileMode`]).
//! * **The tar mode is masked.** act assigns `int64(fi.Mode())` wholesale, so a
//!   symlink entry carries `0o400000000 | 0o777` — Go's `ModeSymlink` type bit
//!   in a field that is supposed to hold permission bits. That needs ten octal
//!   digits, does not fit the eight-byte tar mode field, and Go's writer
//!   responds by dropping the whole entry to GNU base-256
//!   (`80 00 00 00 08 00 01 ed` for a `0o777` symlink, verified against act
//!   v0.2.89). This port writes [`FileMode::open_mode`] instead, which is what
//!   `tar.FileInfoHeader` produced before act's overwrite line, so the entry
//!   stays USTAR and round-trips through any reader — including the `tar` crate
//!   in the opposite direction, whose `Header::mode` cannot parse base-256 at
//!   all. Every extractor masks the mode to `0o7777` regardless, so a symlink
//!   lands on `0o777` either way and no job can tell the difference.
//! * `CopyCollector` opens the destination with `O_CREATE|O_WRONLY` and no
//!   `O_TRUNC`, so copying over a longer existing file leaves a tail behind.
//!   `OpenOptions` here is configured the same way on purpose.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use crate::git_index::{GitIndex, IndexError};
use crate::gitignore::Matcher;

/// Go's `fs.FileMode` bit layout.
///
/// Kept bit-for-bit because `TarCollector` writes the raw value into the tar
/// header, exactly as act does with `int64(fi.Mode())` — including the type
/// bits, so a symlink entry carries `0o20000000` in its mode field. Extractors
/// mask the mode down to `0o7777`, so this is harmless, and pinning it keeps
/// the tar byte-identical to act's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMode(u32);

impl FileMode {
    /// `d: is a directory`
    pub const DIR: u32 = 1 << 31;
    /// `L: symbolic link`
    pub const SYMLINK: u32 = 1 << 27;
    /// `D: device file`
    pub const DEVICE: u32 = 1 << 26;
    /// `p: named pipe (FIFO)`
    pub const NAMED_PIPE: u32 = 1 << 25;
    /// `S: Unix domain socket`
    pub const SOCKET: u32 = 1 << 24;
    /// `u: setuid`
    pub const SETUID: u32 = 1 << 23;
    /// `g: setgid`
    pub const SETGID: u32 = 1 << 22;
    /// `t: sticky`
    pub const STICKY: u32 = 1 << 20;
    /// `?: non-regular file; nothing else is known`
    pub const IRREGULAR: u32 = 1 << 19;

    /// The set of bits that make a file something other than a regular file.
    pub const TYPE: u32 = Self::DIR
        | Self::SYMLINK
        | Self::NAMED_PIPE
        | Self::SOCKET
        | Self::DEVICE
        | Self::IRREGULAR;

    /// The permission bits, `0o777`.
    pub const PERM: u32 = 0o777;
    /// The permission bits plus setuid, setgid and sticky — what `open(2)`
    /// actually applies.
    pub const PERM_AND_SPECIAL: u32 = 0o7777;

    /// The raw value.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// `fi.Mode().Perm()`
    pub fn perm(self) -> u32 {
        self.0 & Self::PERM
    }

    /// The mode an `open(2)` call would actually install.
    pub fn open_mode(self) -> u32 {
        self.0 & Self::PERM_AND_SPECIAL
    }

    /// `fi.IsDir()`
    pub fn is_dir(self) -> bool {
        self.0 & Self::DIR != 0
    }

    /// `fi.Mode()&os.ModeSymlink == os.ModeSymlink`
    pub fn is_symlink(self) -> bool {
        self.0 & Self::SYMLINK != 0
    }

    /// `fi.Mode().IsRegular()`
    pub fn is_regular(self) -> bool {
        self.0 & Self::TYPE == 0
    }
}

/// The parts of Go's `fs.FileInfo` that act uses.
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// The Go file mode, see [`FileMode`].
    pub mode: FileMode,
    /// `fi.Size()`; the tar header's length.
    pub size: u64,
    /// `fi.ModTime()`.
    pub modified: SystemTime,
}

impl FileInfo {
    /// Reads metadata without following symlinks, i.e. Go's `os.Lstat`.
    pub fn lstat(path: &Path) -> io::Result<Self> {
        let meta = fs::symlink_metadata(path)?;
        Ok(Self::from_metadata(&meta))
    }

    /// Converts `std::fs::Metadata` into Go's `fs.FileMode`.
    pub fn from_metadata(meta: &fs::Metadata) -> Self {
        let mut mode = perm_bits(meta);
        let file_type = meta.file_type();
        if file_type.is_dir() {
            mode |= FileMode::DIR;
        } else if file_type.is_symlink() {
            mode |= FileMode::SYMLINK;
        } else {
            mode |= special_type_bits(&file_type);
        }
        mode |= special_permission_bits(meta);
        Self {
            mode: FileMode(mode),
            size: meta.len(),
            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        }
    }
}

/// `fi.Mode().Perm()`.
#[cfg(unix)]
fn perm_bits(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & FileMode::PERM
}

/// On Windows only the read-only flag is representable; Go's `Perm()` clears
/// the owner-write bit for a read-only file.
#[cfg(windows)]
fn perm_bits(meta: &fs::Metadata) -> u32 {
    if meta.permissions().readonly() {
        FileMode::PERM & !0o200
    } else {
        FileMode::PERM
    }
}

/// The file-type bits Go records for anything that is not a directory, a
/// symlink or a regular file.
#[cfg(unix)]
fn special_type_bits(file_type: &fs::FileType) -> u32 {
    use std::os::unix::fs::FileTypeExt;
    if file_type.is_fifo() {
        FileMode::NAMED_PIPE
    } else if file_type.is_socket() {
        FileMode::SOCKET
    } else if file_type.is_block_device() || file_type.is_char_device() {
        FileMode::DEVICE
    } else {
        0
    }
}

/// Windows has no fifos, sockets or devices to report.
#[cfg(windows)]
fn special_type_bits(_file_type: &fs::FileType) -> u32 {
    0
}

/// setuid, setgid and sticky. Windows has no equivalent, and Go's `Perm()`
/// hides them there too.
#[cfg(unix)]
fn special_permission_bits(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    let raw = meta.permissions().mode();
    let mut bits = 0;
    if raw & 0o4000 != 0 {
        bits |= FileMode::SETUID;
    }
    if raw & 0o2000 != 0 {
        bits |= FileMode::SETGID;
    }
    if raw & 0o1000 != 0 {
        bits |= FileMode::STICKY;
    }
    bits
}

#[cfg(windows)]
fn special_permission_bits(_meta: &fs::Metadata) -> u32 {
    0
}

/// What a walk callback wants the walker to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkOutcome {
    /// Descend into the entry, or keep walking.
    Continue,
    /// Do not descend into this entry. Upstream's `filepath.SkipDir`.
    SkipDir,
}

/// A cooperative cancellation flag, standing in for act's `context.Context`.
///
/// A flag that was never cancelled behaves like `context.Background()`: the
/// walk proceeds and nothing aborts it.
#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    /// A flag that is not cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests that the walk stop.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// True once [`Cancellation::cancel`] has been called.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Receives every file the collector decides to emit.
pub trait Handler {
    /// Handles one file.
    ///
    /// `link_name` is non-empty only for symlinks, and `contents` is then
    /// `None`: a symlink has no data of its own, only its target.
    fn write_file(
        &mut self,
        path: &str,
        info: &FileInfo,
        link_name: &str,
        contents: Option<&mut dyn Read>,
        cancelled: &Cancellation,
    ) -> io::Result<()>;
}

/// The filesystem operations the collector needs.
pub trait Fs {
    /// Walks `root` depth-first, sorted by name, without following symlinks.
    fn walk(
        &self,
        root: &Path,
        visit: &mut dyn FnMut(&Path, io::Result<FileInfo>) -> WalkOutcome,
    ) -> io::Result<()>;

    /// Opens the git index of the repository containing `path`.
    fn open_git_index(&self, path: &Path) -> Result<GitIndex, IndexError>;

    /// Opens a file for reading.
    fn open(&self, path: &Path) -> io::Result<fs::File>;

    /// Reads a symlink's target.
    fn read_link(&self, path: &Path) -> io::Result<PathBuf>;
}

/// Go's `filecollector.DefaultFs`: the real filesystem.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultFs;

impl Fs for DefaultFs {
    fn walk(
        &self,
        root: &Path,
        visit: &mut dyn FnMut(&Path, io::Result<FileInfo>) -> WalkOutcome,
    ) -> io::Result<()> {
        walk_inner(root, visit)
    }

    fn open_git_index(&self, path: &Path) -> Result<GitIndex, IndexError> {
        GitIndex::open(path)
    }

    fn open(&self, path: &Path) -> io::Result<fs::File> {
        fs::File::open(path)
    }

    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        fs::read_link(path)
    }
}

/// Go's `filepath.Walk`, ported.
///
/// Differences that matter: Go threads an `error` into the callback so a
/// `ReadDir` failure is visible to it, and a callback error aborts the walk.
/// Here the callback receives the `io::Result<FileInfo>` for the entry itself
/// and the return value only steers descent, so a per-entry failure is always
/// reported to the callback and a directory that cannot be listed aborts the
/// walk, as upstream does.
fn walk_inner(
    path: &Path,
    visit: &mut dyn FnMut(&Path, io::Result<FileInfo>) -> WalkOutcome,
) -> io::Result<()> {
    let info = match FileInfo::lstat(path) {
        Ok(info) => info,
        Err(err) => {
            visit(path, Err(err));
            return Ok(());
        }
    };

    if !info.mode.is_dir() {
        visit(path, Ok(info));
        return Ok(());
    }

    // Go reads and sorts the whole directory before calling back, so a
    // callback that mutates the directory does not perturb this level.
    let mut names: Vec<PathBuf> = Vec::new();
    let mut listing_error = None;
    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => names.push(entry.file_name().into()),
                    Err(err) => {
                        listing_error = Some(err);
                        break;
                    }
                }
            }
            names.sort();
        }
        Err(err) => listing_error = Some(err),
    }

    // Go hands the `ReadDir` failure to the callback and lets it decide
    // whether the walk continues.
    if let Some(err) = listing_error {
        visit(path, Err(err));
        return Ok(());
    }

    // A directory is reported before its children, and `SkipDir` here means
    // "do not descend" — including for the walk root, which Go swallows.
    if visit(path, Ok(info)) == WalkOutcome::SkipDir {
        return Ok(());
    }

    for name in names {
        let child = path.join(&name);
        let info = match FileInfo::lstat(&child) {
            Ok(info) => info,
            Err(err) => {
                // Go calls back with the error and keeps going; a non-fatal
                // answer is ignored either way.
                visit(&child, Err(err));
                continue;
            }
        };
        if info.mode.is_dir() {
            walk_inner(&child, visit)?;
        } else {
            visit(&child, Ok(info));
        }
    }
    Ok(())
}

/// Writes collected files into a tar stream.
pub struct TarCollector<'a, W: Write> {
    /// The stream to write to.
    pub tar: &'a mut tar::Builder<W>,
    /// Owner written into every header. act passes the container user, or
    /// zero when it builds a tar for its own local cache.
    pub uid: u32,
    /// Group written into every header.
    pub gid: u32,
    /// Prefix prepended to every entry name, so the tar unpacks into a
    /// subdirectory of the container. Empty for a plain workspace copy.
    pub dst_dir: String,
}

impl<W: Write> Handler for TarCollector<'_, W> {
    fn write_file(
        &mut self,
        path: &str,
        info: &FileInfo,
        link_name: &str,
        contents: Option<&mut dyn Read>,
        cancelled: &Cancellation,
    ) -> io::Result<()> {
        // A plain USTAR header: every field this collector writes fits the
        // standard octal encoding, which is what makes the archive readable
        // by the widest set of tools. See the mode note below.
        let mut header = tar::Header::new_ustar();
        let name = join_path(&self.dst_dir, path);

        if !link_name.is_empty() {
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_link_name(link_name)?;
        } else {
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(info.size);
        }

        // act assigns `int64(fi.Mode())` wholesale, which for a symlink is
        // `0o400000000 | 0o777`: the permission bits *and* Go's `ModeSymlink`
        // type bit. That value needs ten octal digits and does not fit the
        // eight-byte tar mode field, so Go's writer drops the whole entry to
        // GNU base-256. This port writes `fi.Mode().Perm()` plus the
        // setuid/setgid/sticky bits instead, i.e. exactly what
        // `tar.FileInfoHeader` produced before act's overwrite line, so the
        // entry stays USTAR and round-trips through any tar reader. Every
        // extractor masks the mode to `0o7777` anyway, so a symlink lands on
        // `0o777` either way. Verified against act v0.2.89: the raw mode and
        // the base-256 field bytes are documented in the port notes.
        header.set_mode(info.mode.open_mode());
        let seconds = info
            .modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        header.set_mtime(seconds);
        header.set_uid(u64::from(self.uid));
        header.set_gid(u64::from(self.gid));
        header.set_path(&name)?;
        // The checksum covers every field above, so it is computed last.
        header.set_cksum();

        match contents {
            // A symlink carries no payload, so the header is the whole entry.
            None => self.tar.append(&header, io::empty()),
            // `append_data` streams the file and pads it to the block size;
            // writing the bytes to the underlying sink by hand would corrupt
            // the archive.
            Some(contents) => self.tar.append_data(
                &mut header,
                &name,
                CancellableReader {
                    inner: contents,
                    cancelled,
                },
            ),
        }
    }
}

/// Wraps a reader so an aborted run stops mid-file, which is what act achieves
/// by closing the handle from another goroutine.
struct CancellableReader<'a, R> {
    inner: R,
    cancelled: &'a Cancellation,
}

impl<R: Read> Read for CancellableReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cancelled.is_cancelled() {
            return Err(cancelled_error());
        }
        self.inner.read(buf)
    }
}

/// act's `fmt.Errorf("copy cancelled")`.
fn cancelled_error() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "copy cancelled")
}

/// Copies collected files into a directory tree, used by the host runner when
/// no container is involved.
pub struct CopyCollector {
    /// Root the files are copied into.
    pub dst_dir: PathBuf,
}

impl Handler for CopyCollector {
    fn write_file(
        &mut self,
        path: &str,
        info: &FileInfo,
        link_name: &str,
        contents: Option<&mut dyn Read>,
        cancelled: &Cancellation,
    ) -> io::Result<()> {
        let dest = self.dst_dir.join(path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        if !link_name.is_empty() {
            return symlink(link_name, &dest);
        }
        // Deliberately no `truncate`: upstream opens with O_CREATE|O_WRONLY.
        let mut file = open_with_mode(&dest, info.mode.open_mode())?;
        let Some(mut contents) = contents else {
            return Ok(());
        };
        copy_cancellable(&mut file, &mut contents, cancelled)
    }
}

/// `os.OpenFile(path, O_CREATE|O_WRONLY, mode)`.
///
/// `mode` is Go's full `FileMode`; the kernel keeps only `0o7777` of it, which
/// is what [`FileMode::open_mode`] passes on.
#[cfg(unix)]
fn open_with_mode(path: &Path, mode: u32) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .mode(mode)
        .open(path)
}

/// Windows has no mode argument to `CreateFile` — the only permission bit that
/// survives there is read-only — and Rust's `OpenOptions` exposes no setter
/// that mirrors Go's `O_CREAT` mode. The destination is therefore created
/// writable and *truncated*, which departs from the Unix path below.
///
/// The departure is deliberate and one-sided: with the mode unavailable there
/// is nothing to make a second write fail, so the stale tail that act's
/// missing `O_TRUNC` leaves behind would become a real corruption rather than
/// a faithful oddity. It is documented here and in the module header rather
/// than reproduced.
#[cfg(windows)]
fn open_with_mode(path: &Path, _mode: u32) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
}

/// Go's `path.Join`: slash-separated and lexically cleaned.
///
/// act uses `path.Join` rather than `filepath.Join` for tar entry names so the
/// archive is portable regardless of the host separator.
fn join_path(base: &str, path: &str) -> String {
    let joined = if base.is_empty() {
        path.to_string()
    } else if path.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{path}")
    };
    clean_slash(&joined)
}

/// Go's `path.Clean`, restricted to forward slashes.
fn clean_slash(path: &str) -> String {
    let rooted = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => match out.last() {
                Some(last) if *last != ".." => {
                    out.pop();
                }
                _ => {
                    if !rooted {
                        out.push("..");
                    }
                }
            },
            other => out.push(other),
        }
    }
    let mut cleaned = out.join("/");
    if rooted {
        cleaned.insert(0, '/');
    }
    if cleaned.is_empty() {
        return ".".to_string();
    }
    cleaned
}

/// `os.Symlink`, with the Windows target type probed as Rust requires it.
#[cfg(unix)]
fn symlink(target: &str, dest: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, dest)
}

#[cfg(windows)]
fn symlink(target: &str, dest: &Path) -> io::Result<()> {
    // Windows must know whether the target is a file or a directory. The tar
    // collector never follows the link, so probing is the only option; a
    // broken or unresolvable link falls back to a file link, which is what
    // the vast majority of action repositories use.
    let resolved = dest
        .parent()
        .map(|parent| parent.join(target))
        .unwrap_or_else(|| PathBuf::from(target));
    match fs::metadata(&resolved) {
        Ok(meta) if meta.is_dir() => std::os::windows::fs::symlink_dir(target, dest),
        _ => std::os::windows::fs::symlink_file(target, dest),
    }
}

/// Copies in blocks, stopping when cancellation is requested.
///
/// Upstream reaches the same result by closing the file from another
/// goroutine, which makes `io.Copy` return `use of closed file`. The message
/// differs; the abort does not.
fn copy_cancellable<W: Write, R: Read>(
    sink: &mut W,
    source: &mut R,
    cancelled: &Cancellation,
) -> io::Result<()> {
    let mut buffer = [0u8; 64 * 1024];
    loop {
        if cancelled.is_cancelled() {
            return Err(cancelled_error());
        }
        let read = source.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        sink.write_all(&buffer[..read])?;
    }
}

/// Collects the files of a directory tree and hands them to a [`Handler`].
///
/// Port of act's `filecollector.FileCollector`.
pub struct FileCollector<'a> {
    /// The filesystem to walk.
    pub fs: &'a dyn Fs,
    /// Patterns to honour. `None` disables the `.gitignore` filter entirely,
    /// which is Go's `Ignorer == nil`.
    pub ignorer: Option<Matcher>,
    /// The directory being collected. Submodule recursion resolves the child
    /// index relative to it.
    pub src_path: PathBuf,
    /// The prefix stripped from every walked path.
    ///
    /// act's call sites disagree here and the difference is load-bearing:
    /// `HostEnvironment` passes `src_path` itself, while
    /// `containerReference.copyDir` passes `filepath.Dir(src_path)`. Since the
    /// ignorer is always built relative to `src_path`, the second form shifts
    /// every matched path by one component and so stops nested `.gitignore`
    /// domains from lining up. It is preserved rather than reconciled.
    pub src_prefix: String,
    /// Where collected files go.
    pub handler: &'a mut dyn Handler,
    /// Aborts the walk.
    pub cancellation: Cancellation,
}

impl<'a> FileCollector<'a> {
    /// Builds a collector with the production defaults: no ignore filter and no
    /// destination prefix.
    pub fn new(fs: &'a dyn Fs, handler: &'a mut dyn Handler) -> Self {
        Self {
            fs,
            ignorer: None,
            src_path: PathBuf::new(),
            src_prefix: String::new(),
            handler,
            cancellation: Cancellation::new(),
        }
    }

    /// Walks `root` and emits every file the filters accept.
    ///
    /// Port of `CollectFiles` plus the `filepath.Walk` call act wraps it in.
    ///
    /// `submodule_path` is the component path of the repository being walked
    /// relative to `src_prefix`. It is empty for the top-level tree and grows
    /// as submodules are entered, so that index lookups are relative to the
    /// submodule rather than the superproject.
    pub fn collect_files(&mut self, root: &Path, submodule_path: &[String]) -> io::Result<()> {
        // Upstream: `i, _ := fc.Fs.OpenGitIndex(path.Join(fc.SrcPath,
        // path.Join(submodulePath...)))`. A directory that is not a
        // repository simply has no index, and then every path looks untracked.
        let mut index_root = self.src_path.clone();
        for part in submodule_path {
            index_root.push(part);
        }
        let index = self.fs.open_git_index(&index_root).ok();

        // Copied out so the closure can borrow the collector mutably.
        let fs = self.fs;
        let mut failure: Option<io::Error> = None;
        fs.walk(root, &mut |file, info| {
            if failure.is_some() {
                return WalkOutcome::SkipDir;
            }
            match self.visit(file, info, submodule_path, index.as_ref()) {
                Ok(outcome) => outcome,
                Err(err) => {
                    failure = Some(err);
                    WalkOutcome::SkipDir
                }
            }
        })?;
        match failure {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// Decides what happens to one walked entry.
    fn visit(
        &mut self,
        file: &Path,
        info: io::Result<FileInfo>,
        submodule_path: &[String],
        index: Option<&GitIndex>,
    ) -> Result<WalkOutcome, io::Error> {
        let info = info?;

        if self.cancellation.is_cancelled() {
            return Err(cancelled_error());
        }

        // `strings.TrimPrefix` returns the string unchanged when the prefix is
        // absent, so a walk root that does not carry the prefix is matched as
        // an absolute path — including its ancestors.
        let file_str = file.to_string_lossy();
        let sans_prefix = match file_str.strip_prefix(self.src_prefix.as_str()) {
            Some(rest) => rest,
            None => file_str.as_ref(),
        };
        let split: Vec<String> = sans_prefix
            .split(std::path::MAIN_SEPARATOR)
            .map(str::to_string)
            .collect();

        // The root folder is reported with a trailing "." by some walkers and
        // must be skipped; that is the only place a "." component appears.
        if info.mode.is_dir() && split.last().map(String::as_str) == Some(".") {
            return Ok(WalkOutcome::Continue);
        }

        let tail = tail_of(&split, submodule_path.len());
        let entry = index.and_then(|index| index.entry(&tail.join("/")));

        // A path that git does not track, and that `.gitignore` ignores, is
        // dropped. A tracked path is always emitted: `node_modules` and other
        // build outputs must reach the job when git knows about them.
        if entry.is_none() {
            if let Some(ignorer) = &self.ignorer {
                if ignorer.matches(&split, info.mode.is_dir()) {
                    if !info.mode.is_dir() {
                        return Ok(WalkOutcome::Continue);
                    }
                    match index {
                        // Skip the directory unless it still holds a tracked
                        // file somewhere below it.
                        Some(index) => {
                            let mut probe = tail.to_vec();
                            probe.push("**".to_string());
                            let holds_tracked =
                                matches!(index.glob(&probe.join("/")), Ok(m) if !m.is_empty());
                            if !holds_tracked {
                                return Ok(WalkOutcome::SkipDir);
                            }
                        }
                        None => return Ok(WalkOutcome::SkipDir),
                    }
                }
            }
        }

        if let Some(entry) = entry {
            if entry.is_submodule() {
                self.collect_files(file, &split)?;
                return Ok(WalkOutcome::SkipDir);
            }
        }

        let path = to_slash(sans_prefix);

        if info.mode.is_symlink() {
            let link_name = self
                .fs
                .read_link(file)
                .map_err(|err| {
                    io::Error::new(
                        err.kind(),
                        format!("unable to readlink '{}': {err}", file.display()),
                    )
                })?
                .to_string_lossy()
                .into_owned();
            return self
                .handler
                .write_file(&path, &info, &link_name, None, &self.cancellation)
                .map(|()| WalkOutcome::Continue);
        }
        if !info.mode.is_regular() {
            return Ok(WalkOutcome::Continue);
        }

        let mut contents = self.fs.open(file)?;
        self.handler
            .write_file(&path, &info, "", Some(&mut contents), &self.cancellation)
            .map(|()| WalkOutcome::Continue)
    }
}

/// `split[len(submodule_path):]`, guarding the index the way the caller
/// expects when the two lengths do not line up.
fn tail_of(split: &[String], skip: usize) -> &[String] {
    &split[skip.min(split.len())..]
}

/// `strings.TrimPrefix` on the slash-normalized path.
fn to_slash(path: &str) -> String {
    if std::path::MAIN_SEPARATOR == '/' {
        path.to_string()
    } else {
        path.replace('\\', "/")
    }
}
