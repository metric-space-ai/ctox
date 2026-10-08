#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use crate::options::OptionSet;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::OnceCell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocMode {
    None,
    Trunc,
    Falloc,
    Prealloc,
}

impl AllocMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "none" => AllocMode::None,
            "falloc" => AllocMode::Falloc,
            "prealloc" => AllocMode::Prealloc,
            _ => AllocMode::Trunc,
        }
    }
}

struct Pending {
    ranges: Vec<(u64, Vec<u8>)>,
    bytes: usize,
}

static OPEN_PEAK: AtomicU64 = AtomicU64::new(0);
static LAST_ALLOC: AtomicU64 = AtomicU64::new(0);
static LAST_TRY_PWRITE: AtomicU64 = AtomicU64::new(0);
static LAST_TRY_PREAD: AtomicU64 = AtomicU64::new(0);
static LAST_CACHE_PWRITE: AtomicU64 = AtomicU64::new(0);
static LAST_TRY_CACHE: AtomicU64 = AtomicU64::new(0);
static LAST_SYNC_OPEN: AtomicU64 = AtomicU64::new(0);
static LAST_SYNC_TRUNC: AtomicU64 = AtomicU64::new(0);
static LAST_SYNC_FALLOC: AtomicU64 = AtomicU64::new(0);
static LAST_SYNC_PREALLOC: AtomicU64 = AtomicU64::new(0);
static LAST_MKDIR: AtomicU64 = AtomicU64::new(0);
static LAST_FILE_WRITE: AtomicU64 = AtomicU64::new(0);
static LAST_FILE_READ: AtomicU64 = AtomicU64::new(0);
static LAST_UNLINK: AtomicU64 = AtomicU64::new(0);

pub fn last_open_peak() -> u64 {
    OPEN_PEAK.load(Ordering::SeqCst)
}

pub fn reset_open_peak() {
    OPEN_PEAK.store(0, Ordering::SeqCst);
}

pub fn last_try_pwrite() -> u64 {
    LAST_TRY_PWRITE.load(Ordering::SeqCst)
}

pub fn reset_try_pwrite() {
    LAST_TRY_PWRITE.store(0, Ordering::SeqCst);
}

pub fn last_try_pread() -> u64 {
    LAST_TRY_PREAD.load(Ordering::SeqCst)
}

pub fn reset_try_pread() {
    LAST_TRY_PREAD.store(0, Ordering::SeqCst);
}

pub fn last_cache_pwrite() -> u64 {
    LAST_CACHE_PWRITE.load(Ordering::SeqCst)
}

pub fn reset_cache_pwrite() {
    LAST_CACHE_PWRITE.store(0, Ordering::SeqCst);
}

pub fn last_try_cache() -> u64 {
    LAST_TRY_CACHE.load(Ordering::SeqCst)
}

pub fn reset_try_cache() {
    LAST_TRY_CACHE.store(0, Ordering::SeqCst);
}

pub fn last_sync_open() -> u64 {
    LAST_SYNC_OPEN.load(Ordering::SeqCst)
}

pub fn reset_sync_open() {
    LAST_SYNC_OPEN.store(0, Ordering::SeqCst);
}

pub fn last_sync_trunc() -> u64 {
    LAST_SYNC_TRUNC.load(Ordering::SeqCst)
}

pub fn reset_sync_trunc() {
    LAST_SYNC_TRUNC.store(0, Ordering::SeqCst);
}

pub fn last_sync_falloc() -> u64 {
    LAST_SYNC_FALLOC.load(Ordering::SeqCst)
}

pub fn reset_sync_falloc() {
    LAST_SYNC_FALLOC.store(0, Ordering::SeqCst);
}

pub fn last_sync_prealloc() -> u64 {
    LAST_SYNC_PREALLOC.load(Ordering::SeqCst)
}

pub fn reset_sync_prealloc() {
    LAST_SYNC_PREALLOC.store(0, Ordering::SeqCst);
}

pub fn last_mkdir() -> u64 {
    LAST_MKDIR.load(Ordering::SeqCst)
}

pub fn reset_mkdir() {
    LAST_MKDIR.store(0, Ordering::SeqCst);
}

/// C++ File::mkdirs: dest parents on the download thread.
pub fn mkdirs(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(path).map_err(|e| Error::Other(e.to_string()))?;
    LAST_MKDIR.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// C++ File::write: session / follow dest on the download thread.
pub fn write_file(path: &Path, data: impl AsRef<[u8]>) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            mkdirs(parent)?;
        }
    }
    std::fs::write(path, data.as_ref()).map_err(|e| Error::Other(e.to_string()))?;
    LAST_FILE_WRITE.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// C++ File::read: input-file / follow-torrent dest on the download thread.
pub fn read_file(path: &Path) -> Result<Vec<u8>> {
    let b = std::fs::read(path)?;
    LAST_FILE_READ.fetch_add(1, Ordering::Relaxed);
    Ok(b)
}

/// C++ File::remove: unselected BT dest / follow-torrent=mem.
pub fn unlink(path: &Path) -> bool {
    match std::fs::remove_file(path) {
        Ok(()) => {
            LAST_UNLINK.fetch_add(1, Ordering::Relaxed);
            true
        }
        Err(_) => false,
    }
}

pub fn last_file_write() -> u64 {
    LAST_FILE_WRITE.load(Ordering::SeqCst)
}

pub fn reset_file_write() {
    LAST_FILE_WRITE.store(0, Ordering::SeqCst);
}

pub fn last_file_read() -> u64 {
    LAST_FILE_READ.load(Ordering::SeqCst)
}

pub fn reset_file_read() {
    LAST_FILE_READ.store(0, Ordering::SeqCst);
}

pub fn last_unlink() -> u64 {
    LAST_UNLINK.load(Ordering::SeqCst)
}

pub fn reset_unlink() {
    LAST_UNLINK.store(0, Ordering::SeqCst);
}

pub fn last_alloc_kind() -> &'static str {
    match LAST_ALLOC.load(Ordering::SeqCst) {
        2 => "falloc",
        3 => "prealloc",
        1 => "trunc",
        _ => "none",
    }
}

fn store_alloc(mode: AllocMode) {
    let n = match mode {
        AllocMode::None => 0,
        AllocMode::Trunc => 1,
        AllocMode::Falloc => 2,
        AllocMode::Prealloc => 3,
    };
    LAST_ALLOC.store(n, Ordering::SeqCst);
}

/// One positioned `pwrite` (unix). Windows `seek_write` also moves the file
/// cursor; every caller in this crate does positional IO only, so that is unobservable.
#[cfg(unix)]
fn pwrite_once(f: &std::fs::File, data: &[u8], off: u64) -> std::io::Result<usize> {
    rustix::io::retry_on_intr(|| rustix::io::pwrite(f, data, off)).map_err(std::io::Error::from)
}

#[cfg(windows)]
fn pwrite_once(f: &std::fs::File, data: &[u8], off: u64) -> std::io::Result<usize> {
    std::os::windows::fs::FileExt::seek_write(f, data, off)
}

/// One positioned `pread` (unix); Windows `seek_read`, see [`pwrite_once`].
#[cfg(unix)]
pub(crate) fn pread_once(f: &std::fs::File, buf: &mut [u8], off: u64) -> std::io::Result<usize> {
    rustix::io::retry_on_intr(|| rustix::io::pread(f, &mut *buf, off)).map_err(std::io::Error::from)
}

#[cfg(windows)]
pub(crate) fn pread_once(f: &std::fs::File, buf: &mut [u8], off: u64) -> std::io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(f, buf, off)
}

/// C++ `DirectDiskWriter` / `DefaultDiskWriter`: positioned `pwrite` on the
/// download thread (no per-chunk copy / `spawn_blocking`), no per-chunk `fflush`.
fn pwrite_all(f: &std::fs::File, mut data: &[u8], mut off: u64) -> std::io::Result<()> {
    while !data.is_empty() {
        let n = pwrite_once(f, data, off)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "pwrite",
            ));
        }
        data = &data[n..];
        off += n as u64;
    }
    Ok(())
}

fn pread_all(f: &std::fs::File, buf: &mut [u8], mut off: u64) -> std::io::Result<usize> {
    let mut n = 0usize;
    while n < buf.len() {
        let r = pread_once(f, &mut buf[n..], off)?;
        if r == 0 {
            break;
        }
        n += r;
        off += r as u64;
    }
    Ok(n)
}

/// C++ DefaultDiskWriter::readDataInternal: positioned `pread` dest fd.
pub fn pread_at(path: &Path, buf: &mut [u8], off: u64) -> Result<usize> {
    let f = std::fs::OpenOptions::new().read(true).open(path)?;
    let n = pread_all(&f, buf, off)?;
    LAST_TRY_PREAD.fetch_add(1, Ordering::Relaxed);
    Ok(n)
}

fn open_dest(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)
}

/// C++ `--bt-max-open-files` (default 100): cap simultaneously open dest files.
/// C++ MultiDiskAdaptor: last DiskWriter is reused without a map lookup.
pub struct FilePool {
    max: usize,
    inner: std::sync::Mutex<PoolInner>,
    last: std::sync::Mutex<Option<(PathBuf, Arc<std::fs::File>)>>,
    last_hits: AtomicU64,
    lookups: AtomicU64,
    map_hits: AtomicU64,
}

struct PoolInner {
    files: HashMap<PathBuf, Arc<std::fs::File>>,
    /// C++ MultiDiskAdaptor open-file LRU: generation stamp (lazy, O(1) touch).
    lru: VecDeque<(PathBuf, u64)>,
    gen: HashMap<PathBuf, u64>,
    next_gen: u64,
}

impl FilePool {
    pub fn new(max: usize) -> Arc<Self> {
        Arc::new(Self {
            max,
            inner: std::sync::Mutex::new(PoolInner {
                files: HashMap::new(),
                lru: VecDeque::new(),
                gen: HashMap::new(),
                next_gen: 0,
            }),
            last: std::sync::Mutex::new(None),
            last_hits: AtomicU64::new(0),
            lookups: AtomicU64::new(0),
            map_hits: AtomicU64::new(0),
        })
    }

    pub fn last_hits(&self) -> u64 {
        self.last_hits.load(Ordering::SeqCst)
    }

    pub fn lookups(&self) -> u64 {
        self.lookups.load(Ordering::SeqCst)
    }

    pub fn map_hits(&self) -> u64 {
        self.map_hits.load(Ordering::SeqCst)
    }

    fn touch_lru(g: &mut PoolInner, path: &Path) {
        g.next_gen = g.next_gen.wrapping_add(1);
        let gen = g.next_gen;
        g.gen.insert(path.to_path_buf(), gen);
        g.lru.push_back((path.to_path_buf(), gen));
    }

    fn evict_to_cap(g: &mut PoolInner, max: usize) {
        while max > 0 && g.files.len() >= max {
            let Some((old, gen)) = g.lru.pop_front() else {
                break;
            };
            if g.gen.get(&old) == Some(&gen) {
                g.files.remove(&old);
                g.gen.remove(&old);
            }
        }
    }

    fn last_hit(&self, path: &Path) -> Option<Arc<std::fs::File>> {
        let g = self.last.lock().ok()?;
        match g.as_ref() {
            Some((p, f)) if p.as_path() == path => Some(Arc::clone(f)),
            _ => None,
        }
    }

    fn remember(&self, path: &Path, f: &Arc<std::fs::File>) {
        if let Ok(mut g) = self.last.lock() {
            *g = Some((path.to_path_buf(), Arc::clone(f)));
        }
    }

    fn try_pwrite_last(&self, path: &Path, offset: u64, data: &[u8]) -> Result<bool> {
        if let Some(f) = self.last_hit(path) {
            self.last_hits.fetch_add(1, Ordering::Relaxed);
            pwrite_all(&f, data, offset)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// C++ MultiDiskAdaptor::writeData: last DiskWriter, else open-file map (no event loop).
    fn try_pwrite_pool(&self, path: &Path, offset: u64, data: &[u8]) -> Result<bool> {
        if self.try_pwrite_last(path, offset, data)? {
            return Ok(true);
        }
        self.lookups.fetch_add(1, Ordering::SeqCst);
        let f = {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            let Some(f) = g.files.get(path).cloned() else {
                return Ok(false);
            };
            Self::touch_lru(&mut g, path);
            f
        };
        self.map_hits.fetch_add(1, Ordering::SeqCst);
        self.remember(path, &f);
        pwrite_all(&f, data, offset)?;
        Ok(true)
    }

    fn try_pread_last(&self, path: &Path, offset: u64, buf: &mut [u8]) -> Result<bool> {
        if let Some(f) = self.last_hit(path) {
            self.last_hits.fetch_add(1, Ordering::Relaxed);
            let n = pread_all(&f, buf, offset)?;
            LAST_TRY_PREAD.fetch_add(1, Ordering::Relaxed);
            return Ok(n == buf.len());
        }
        Ok(false)
    }

    /// C++ MultiDiskAdaptor::readData: last DiskWriter, else open-file map (no event loop).
    fn try_pread_pool(&self, path: &Path, offset: u64, buf: &mut [u8]) -> Result<bool> {
        if self.try_pread_last(path, offset, buf)? {
            return Ok(true);
        }
        self.lookups.fetch_add(1, Ordering::SeqCst);
        let f = {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            let Some(f) = g.files.get(path).cloned() else {
                return Ok(false);
            };
            Self::touch_lru(&mut g, path);
            f
        };
        self.map_hits.fetch_add(1, Ordering::SeqCst);
        self.remember(path, &f);
        let n = pread_all(&f, buf, offset)?;
        LAST_TRY_PREAD.fetch_add(1, Ordering::Relaxed);
        Ok(n == buf.len())
    }

    /// C++ MultiDiskAdaptor::readDataOffset: pread dest fd (openFile on miss).
    pub fn read_at(&self, path: &Path, offset: u64, buf: &mut [u8]) -> Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        if self.try_pread_pool(path, offset, buf)? {
            return Ok(());
        }
        let p = path.to_path_buf();
        let opened = open_dest(&p).map_err(|e| Error::Other(e.to_string()))?;
        LAST_SYNC_OPEN.fetch_add(1, Ordering::Relaxed);
        let f = Arc::new(opened);
        {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = g.files.get(path).cloned() {
                Self::touch_lru(&mut g, path);
                self.map_hits.fetch_add(1, Ordering::SeqCst);
                drop(g);
                self.remember(path, &existing);
                let n = pread_all(&existing, buf, offset)?;
                LAST_TRY_PREAD.fetch_add(1, Ordering::Relaxed);
                if n != buf.len() {
                    return Err(Error::Other("short dest read".into()));
                }
                return Ok(());
            }
            Self::evict_to_cap(&mut g, self.max);
            Self::touch_lru(&mut g, &p);
            g.files.insert(p.clone(), Arc::clone(&f));
            let n = g.files.len() as u64;
            OPEN_PEAK.fetch_max(n, Ordering::SeqCst);
        }
        self.remember(path, &f);
        let n = pread_all(&f, buf, offset)?;
        LAST_TRY_PREAD.fetch_add(1, Ordering::Relaxed);
        if n != buf.len() {
            return Err(Error::Other("short dest read".into()));
        }
        Ok(())
    }

    async fn write_at(&self, path: &Path, offset: u64, data: &[u8]) -> Result<()> {
        if self.try_pwrite_pool(path, offset, data)? {
            return Ok(());
        }
        // C++ MultiDiskAdaptor::openFile: POSIX open on the download thread (not spawn_blocking).
        let p = path.to_path_buf();
        let opened = open_dest(&p).map_err(|e| Error::Other(e.to_string()))?;
        LAST_SYNC_OPEN.fetch_add(1, Ordering::Relaxed);
        let f = Arc::new(opened);
        {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = g.files.get(path).cloned() {
                Self::touch_lru(&mut g, path);
                self.map_hits.fetch_add(1, Ordering::SeqCst);
                drop(g);
                self.remember(path, &existing);
                pwrite_all(&existing, data, offset)?;
                return Ok(());
            }
            Self::evict_to_cap(&mut g, self.max);
            Self::touch_lru(&mut g, &p);
            g.files.insert(p.clone(), Arc::clone(&f));
            let n = g.files.len() as u64;
            OPEN_PEAK.fetch_max(n, Ordering::SeqCst);
        }
        self.remember(path, &f);
        pwrite_all(&f, data, offset)?;
        Ok(())
    }
}

pub struct FileStorage {
    path: PathBuf,
    total: u64,
    alloc: AllocMode,
    cache_limit: usize,
    max_write_length: Option<u64>,
    pending: std::sync::Mutex<Pending>,
    writes: AtomicU64,
    opens: AtomicU64,
    held: OnceCell<Arc<std::fs::File>>,
    pool: Option<Arc<FilePool>>,
}

impl FileStorage {
    pub fn new(path: PathBuf, total: u64, alloc: AllocMode) -> Self {
        Self::with_cache(path, total, alloc, 0)
    }

    pub fn from_opts(path: PathBuf, total: u64, alloc: AllocMode, opts: &OptionSet) -> Self {
        let mut storage = Self::with_cache(path, total, alloc, disk_cache_limit(opts));
        storage.max_write_length = opts.get("ctox-expected-length").and_then(|v| v.parse().ok());
        storage
    }

    pub fn with_cache(path: PathBuf, total: u64, alloc: AllocMode, cache_limit: usize) -> Self {
        Self {
            path,
            total,
            alloc,
            cache_limit,
            max_write_length: None,
            pending: std::sync::Mutex::new(Pending {
                ranges: Vec::new(),
                bytes: 0,
            }),
            writes: AtomicU64::new(0),
            opens: AtomicU64::new(0),
            held: OnceCell::new(),
            pool: None,
        }
    }

    pub fn with_pool(mut self, pool: Arc<FilePool>) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn write_count(&self) -> u64 {
        self.writes.load(Ordering::Relaxed)
    }

    pub fn open_count(&self) -> u64 {
        self.opens.load(Ordering::Relaxed)
    }

    async fn open_held(&self) -> Result<Arc<std::fs::File>> {
        // C++ DefaultDiskWriter::openFile: POSIX open on the download thread.
        if let Some(f) = self.held.get() {
            return Ok(Arc::clone(f));
        }
        let f = Arc::new(open_dest(&self.path).map_err(|e| Error::Other(e.to_string()))?);
        match self.held.set(Arc::clone(&f)) {
            Ok(()) => {
                self.opens.fetch_add(1, Ordering::SeqCst);
                LAST_SYNC_OPEN.fetch_add(1, Ordering::Relaxed);
                Ok(f)
            }
            Err(_) => Ok(Arc::clone(self.held.get().expect("held set raced"))),
        }
    }

    pub async fn ensure(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                mkdirs(parent)?;
            }
        }
        match self.alloc {
            AllocMode::None => {
                self.open_held().await?;
                store_alloc(AllocMode::None);
            }
            AllocMode::Trunc => {
                let f = self.open_held().await?;
                if self.total > 0 {
                    // C++ DefaultDiskWriter::truncate: ftruncate dest fd on the download thread.
                    f.set_len(self.total).map_err(|e| Error::Other(e.to_string()))?;
                    LAST_SYNC_TRUNC.fetch_add(1, Ordering::Relaxed);
                }
                store_alloc(AllocMode::Trunc);
            }
            AllocMode::Falloc => {
                let f = self.open_held().await?;
                if self.total > 0 {
                    // C++ FileAllocationIterator: posix_fallocate on dest fd.
                    #[cfg(unix)]
                    rustix::fs::fallocate(
                        &*f,
                        rustix::fs::FallocateFlags::empty(),
                        0,
                        self.total,
                    )
                    .map_err(|e| Error::Other(format!("posix_fallocate: {e}")))?;
                    // No posix_fallocate on Windows: extending the length makes NTFS
                    // allocate the clusters, which is what falloc is for.
                    #[cfg(windows)]
                    f.set_len(self.total)
                        .map_err(|e| Error::Other(format!("falloc set_len: {e}")))?;
                    LAST_SYNC_FALLOC.fetch_add(1, Ordering::Relaxed);
                }
                store_alloc(AllocMode::Falloc);
            }
            AllocMode::Prealloc => {
                let f = self.open_held().await?;
                if self.total > 0 {
                    // C++ FileAllocationIterator PREALLOC: 16KiB zero writes on dest fd.
                    f.set_len(self.total).map_err(|e| Error::Other(e.to_string()))?;
                    let zeros = [0u8; 16 * 1024];
                    let mut off = 0u64;
                    let mut left = self.total;
                    while left > 0 {
                        let n = (left as usize).min(zeros.len());
                        pwrite_all(&f, &zeros[..n], off)
                            .map_err(|e| Error::Other(e.to_string()))?;
                        off += n as u64;
                        left -= n as u64;
                    }
                    LAST_SYNC_PREALLOC.fetch_add(1, Ordering::Relaxed);
                }
                store_alloc(AllocMode::Prealloc);
            }
        }
        Ok(())
    }

    /// C++ DefaultDiskWriter::writeData: pwrite dest fd when already open (no await, no copy).
    pub fn try_pwrite(&self, offset: u64, data: &[u8]) -> Result<bool> {
        self.check_write_length(offset, data.len())?;
        if data.is_empty() {
            return Ok(true);
        }
        if self.cache_limit != 0 {
            return Ok(false);
        }
        let ok = if let Some(pool) = &self.pool {
            pool.try_pwrite_pool(&self.path, offset, data)?
        } else if let Some(f) = self.held.get() {
            pwrite_all(f, data, offset)?;
            true
        } else {
            false
        };
        if ok {
            self.writes.fetch_add(1, Ordering::Relaxed);
            LAST_TRY_PWRITE.fetch_add(1, Ordering::Relaxed);
        }
        Ok(ok)
    }

    /// C++ DefaultDiskWriter::readData: lock-free pread when dest fd is open.
    pub fn try_pread(&self, offset: u64, buf: &mut [u8]) -> Result<bool> {
        if buf.is_empty() {
            return Ok(true);
        }
        let ok = if let Some(pool) = &self.pool {
            pool.try_pread_pool(&self.path, offset, buf)?
        } else if let Some(f) = self.held.get() {
            let n = pread_all(f, buf, offset)?;
            if n == buf.len() {
                LAST_TRY_PREAD.fetch_add(1, Ordering::Relaxed);
                true
            } else {
                false
            }
        } else {
            false
        };
        Ok(ok)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// C++ CheckIntegrityCommand / DefaultDiskWriter::readData: pread dest fd (short OK at EOF).
    pub fn pread_held(&self, offset: u64, buf: &mut [u8]) -> Result<Option<usize>> {
        if buf.is_empty() {
            return Ok(Some(0));
        }
        if let Some(pool) = &self.pool {
            if let Some(f) = pool.last_hit(&self.path) {
                let n = pread_all(&f, buf, offset)?;
                LAST_TRY_PREAD.fetch_add(1, Ordering::Relaxed);
                return Ok(Some(n));
            }
        }
        let Some(f) = self.held.get() else {
            return Ok(None);
        };
        let n = pread_all(f, buf, offset)?;
        LAST_TRY_PREAD.fetch_add(1, Ordering::Relaxed);
        Ok(Some(n))
    }

    /// C++ WrDiskCache::writeCache: copy socket window into cache (no await, no extra Vec).
    pub fn try_cache(&self, offset: u64, data: &[u8]) -> Result<bool> {
        self.check_write_length(offset, data.len())?;
        if data.is_empty() {
            return Ok(true);
        }
        if self.cache_limit == 0 {
            return Ok(false);
        }
        let mut g = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if g.bytes.saturating_add(data.len()) >= self.cache_limit {
            return Ok(false);
        }
        merge_insert(&mut g.ranges, offset, data);
        g.bytes = g.ranges.iter().map(|(_, d)| d.len()).sum();
        LAST_TRY_CACHE.fetch_add(1, Ordering::Relaxed);
        Ok(true)
    }

    /// C++ DefaultDiskWriter::writeData: lock-free pwrite when dest fd is open.
    fn check_write_length(&self, offset: u64, length: usize) -> Result<()> {
        if let Some(limit) = self.max_write_length {
            if offset.checked_add(length as u64).is_none_or(|end| end > limit) {
                return Err(Error::Other("body exceeds pinned content length".into()));
            }
        }
        Ok(())
    }

    pub async fn write_body(&self, offset: u64, data: &[u8]) -> Result<()> {
        self.check_write_length(offset, data.len())?;
        if self.try_pwrite(offset, data)? {
            return Ok(());
        }
        if self.try_cache(offset, data)? {
            return Ok(());
        }
        self.write_at(offset, data).await
    }

    pub async fn write_at(&self, offset: u64, data: &[u8]) -> Result<()> {
        self.check_write_length(offset, data.len())?;
        if data.is_empty() {
            return Ok(());
        }
        if self.cache_limit == 0 {
            self.disk_write(offset, data).await?;
            return Ok(());
        }
        let flush = {
            let mut g = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            merge_insert(&mut g.ranges, offset, data);
            g.bytes = g.ranges.iter().map(|(_, d)| d.len()).sum();
            if g.bytes >= self.cache_limit {
                g.bytes = 0;
                Some(std::mem::take(&mut g.ranges))
            } else {
                None
            }
        };
        if let Some(ranges) = flush {
            for (off, buf) in ranges {
                self.flush_range(off, &buf).await?;
            }
        }
        Ok(())
    }

    pub async fn flush(&self) -> Result<()> {
        if self.cache_limit == 0 {
            return Ok(());
        }
        let ranges = {
            let mut g = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            g.bytes = 0;
            std::mem::take(&mut g.ranges)
        };
        for (off, buf) in ranges {
            self.flush_range(off, &buf).await?;
        }
        Ok(())
    }

    pub async fn discard(&self) {
        if self.cache_limit == 0 {
            return;
        }
        let mut g = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        g.ranges.clear();
        g.bytes = 0;
    }

    /// C++ WrDiskCache::flush → DefaultDiskWriter::writeDataInternal (sync pwrite).
    fn pwrite_now(&self, offset: u64, data: &[u8]) -> Result<bool> {
        if data.is_empty() {
            return Ok(true);
        }
        let ok = if let Some(pool) = &self.pool {
            pool.try_pwrite_pool(&self.path, offset, data)?
        } else if let Some(f) = self.held.get() {
            pwrite_all(f, data, offset)?;
            true
        } else {
            false
        };
        if ok {
            self.writes.fetch_add(1, Ordering::Relaxed);
            LAST_CACHE_PWRITE.fetch_add(1, Ordering::Relaxed);
        }
        Ok(ok)
    }

    async fn flush_range(&self, offset: u64, data: &[u8]) -> Result<()> {
        if self.pwrite_now(offset, data)? {
            return Ok(());
        }
        self.disk_write(offset, data).await
    }

    async fn disk_write(&self, offset: u64, data: &[u8]) -> Result<()> {
        if let Some(pool) = &self.pool {
            pool.write_at(&self.path, offset, data).await?;
            self.writes.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        if let Some(f) = self.held.get() {
            pwrite_all(f, data, offset)?;
            self.writes.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        let f = self.open_held().await?;
        pwrite_all(&f, data, offset)?;
        self.writes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

pub fn disk_cache_limit(opts: &OptionSet) -> usize {
    opts.get("disk-cache")
        .and_then(parse_size)
        .unwrap_or(16 * 1024 * 1024) as usize
}

/// C++ `--no-file-allocation-limit=SIZE` (default 5M): skip allocation when total < SIZE.
pub fn alloc_mode_for(opts: &OptionSet, total: u64) -> AllocMode {
    let mode = AllocMode::parse(opts.get("file-allocation").unwrap_or("trunc"));
    if matches!(mode, AllocMode::None) {
        return AllocMode::None;
    }
    let limit = opts
        .get("no-file-allocation-limit")
        .and_then(parse_size)
        .unwrap_or(5 * 1024 * 1024);
    if total < limit {
        AllocMode::None
    } else {
        mode
    }
}

pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num, mul) = match s.as_bytes().last() {
        Some(b'K' | b'k') => (&s[..s.len() - 1], 1024u64),
        Some(b'M' | b'm') => (&s[..s.len() - 1], 1024 * 1024),
        Some(b'G' | b'g') => (&s[..s.len() - 1], 1024 * 1024 * 1024),
        _ => (s, 1u64),
    };
    num.trim().parse::<u64>().ok().map(|n| n.saturating_mul(mul))
}

/// C++ WrDiskCacheEntry: sequential socket windows append; overlap is the rare path.
fn merge_insert(ranges: &mut Vec<(u64, Vec<u8>)>, off: u64, data: &[u8]) {
    if data.is_empty() {
        return;
    }
    let end = off + data.len() as u64;

    if let Some(i) = ranges
        .iter()
        .position(|(r0, buf)| *r0 + buf.len() as u64 == off)
    {
        ranges[i].1.extend_from_slice(data);
        loop {
            let new_end = ranges[i].0 + ranges[i].1.len() as u64;
            if i + 1 >= ranges.len() {
                break;
            }
            let n0 = ranges[i + 1].0;
            if n0 > new_end {
                break;
            }
            let nxt = ranges.remove(i + 1);
            if n0 == new_end {
                ranges[i].1.extend_from_slice(&nxt.1);
                continue;
            }
            let skip = (new_end - n0) as usize;
            if skip < nxt.1.len() {
                ranges[i].1.extend_from_slice(&nxt.1[skip..]);
            }
        }
        return;
    }

    if let Some(i) = ranges.iter().position(|(r0, _)| *r0 == end) {
        let mut nbuf = Vec::with_capacity(data.len() + ranges[i].1.len());
        nbuf.extend_from_slice(data);
        nbuf.extend_from_slice(&ranges[i].1);
        ranges[i].0 = off;
        ranges[i].1 = nbuf;
        return;
    }

    let overlaps = ranges.iter().any(|(r0, buf)| {
        let r1 = *r0 + buf.len() as u64;
        r1 > off && *r0 < end
    });
    if !overlaps {
        let pos = ranges.partition_point(|(o, _)| *o < off);
        ranges.insert(pos, (off, data.to_vec()));
        return;
    }

    let mut start = off;
    let mut end = end;
    let mut buf = data.to_vec();
    let mut i = 0;
    while i < ranges.len() {
        let r0 = ranges[i].0;
        let r1 = r0 + ranges[i].1.len() as u64;
        if r1 < start || r0 > end {
            i += 1;
            continue;
        }
        let nstart = r0.min(start);
        let nend = r1.max(end);
        let mut nbuf = vec![0u8; (nend - nstart) as usize];
        let rdata = &ranges[i].1;
        nbuf[(r0 - nstart) as usize..][..rdata.len()].copy_from_slice(rdata);
        nbuf[(start - nstart) as usize..][..buf.len()].copy_from_slice(&buf);
        start = nstart;
        end = nend;
        buf = nbuf;
        ranges.remove(i);
    }
    let pos = ranges.partition_point(|(o, _)| *o < start);
    ranges.insert(pos, (start, buf));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_kmg() {
        assert_eq!(parse_size("0"), Some(0));
        assert_eq!(parse_size("16M"), Some(16 * 1024 * 1024));
        assert_eq!(parse_size("4K"), Some(4096));
    }

    #[test]
    fn wrdiskcache_sequential_append_coalesces_without_recopy_quadratic() {
        // C++ WrDiskCacheEntry::writeCache appends the socket window; 16MiB of 64KiB
        // slices must stay one range (the old merge recopied the whole buffer each time).
        let mut ranges: Vec<(u64, Vec<u8>)> = Vec::new();
        let chunk = vec![0xABu8; 64 * 1024];
        let n = 256u64;
        for i in 0..n {
            merge_insert(&mut ranges, i * chunk.len() as u64, &chunk);
        }
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].0, 0);
        assert_eq!(ranges[0].1.len(), n as usize * chunk.len());
        merge_insert(&mut ranges, 32, &[0xFF]);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].1[32], 0xFF);
        let tail = ranges[0].1.len() as u64;
        merge_insert(&mut ranges, tail + 4096, &[1, 2, 3]);
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[1], (tail + 4096, vec![1, 2, 3]));
    }

    #[test]
    fn no_file_allocation_limit_skips_small() {
        let mut opts = OptionSet::new();
        opts.set("file-allocation", "trunc");
        assert!(matches!(alloc_mode_for(&opts, 100), AllocMode::None));
        opts.set("no-file-allocation-limit", "1K");
        assert!(matches!(alloc_mode_for(&opts, 512), AllocMode::None));
        assert!(matches!(alloc_mode_for(&opts, 1024), AllocMode::Trunc));
        opts.set("no-file-allocation-limit", "0");
        assert!(matches!(alloc_mode_for(&opts, 1), AllocMode::Trunc));
        opts.set("file-allocation", "none");
        assert!(matches!(alloc_mode_for(&opts, 10 * 1024 * 1024), AllocMode::None));
        opts.set("file-allocation", "falloc");
        opts.set("no-file-allocation-limit", "0");
        assert!(matches!(alloc_mode_for(&opts, 4096), AllocMode::Falloc));
        opts.set("file-allocation", "prealloc");
        assert!(matches!(alloc_mode_for(&opts, 4096), AllocMode::Prealloc));
    }

    #[tokio::test]
    async fn disk_cache_zero_writes_each_chunk_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("z.bin");
        let st = FileStorage::with_cache(path.clone(), 8, AllocMode::None, 0);
        st.ensure().await.unwrap();
        for i in 0..8u8 {
            st.write_at(i as u64, &[i]).await.unwrap();
        }
        st.flush().await.unwrap();
        assert_eq!(st.write_count(), 8);
        assert_eq!(std::fs::read(&path).unwrap(), (0..8u8).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn disk_cache_coalesces_adjacent_one_write_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.bin");
        let st = FileStorage::with_cache(path.clone(), 8, AllocMode::None, 16);
        st.ensure().await.unwrap();
        for i in 0..8u8 {
            st.write_at(i as u64, &[i]).await.unwrap();
        }
        assert_eq!(st.write_count(), 0, "disk-cache must hold until flush");
        reset_cache_pwrite();
        st.flush().await.unwrap();
        assert_eq!(st.write_count(), 1, "adjacent writes must coalesce");
        assert_eq!(last_cache_pwrite(), 1, "C++ WrDiskCache flush is one dest pwrite");
        assert_eq!(std::fs::read(&path).unwrap(), (0..8u8).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn wrdiskcache_try_cache_socket_window_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.bin");
        let st = FileStorage::with_cache(path.clone(), 8, AllocMode::None, 16);
        st.ensure().await.unwrap();
        reset_try_cache();
        st.write_body(0, b"ABCD").await.unwrap();
        st.write_body(4, b"EFGH").await.unwrap();
        assert_eq!(last_try_cache(), 2, "C++ WrDiskCache: socket window cached");
        assert_eq!(st.write_count(), 0, "must not pwrite until flush");
        st.flush().await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"ABCDEFGH");
    }

    #[tokio::test]
    async fn disk_cache_flushes_when_full_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.bin");
        let st = FileStorage::with_cache(path.clone(), 6, AllocMode::None, 4);
        st.ensure().await.unwrap();
        reset_cache_pwrite();
        st.write_at(0, &[1, 2]).await.unwrap();
        st.write_at(2, &[3, 4]).await.unwrap();
        assert!(st.write_count() >= 1, "full cache must flush to disk");
        assert!(
            last_cache_pwrite() > 0,
            "C++ WrDiskCache flush must pwrite dest fd without tokio mutex"
        );
        st.write_at(4, &[5, 6]).await.unwrap();
        st.flush().await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), vec![1, 2, 3, 4, 5, 6]);
    }

    #[tokio::test]
    async fn keep_dest_fd_pwrite_out_of_order_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.bin");
        let st = FileStorage::with_cache(path.clone(), 16, AllocMode::Trunc, 0);
        reset_sync_open();
        reset_sync_trunc();
        st.ensure().await.unwrap();
        assert_eq!(st.open_count(), 1, "ensure must open dest once");
        assert_eq!(
            last_sync_open(),
            1,
            "C++ DefaultDiskWriter::openFile is POSIX open on the download thread"
        );
        assert_eq!(
            last_sync_trunc(),
            1,
            "C++ DefaultDiskWriter::truncate is ftruncate on dest fd"
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            16,
            "trunc must size dest before first pwrite"
        );
        // C++ DefaultDiskWriter pwrite: later range first, then prefix — dest-match full file.
        st.write_at(12, &[9, 9, 9, 9]).await.unwrap();
        st.write_at(0, &[1, 1, 1, 1]).await.unwrap();
        st.write_at(8, &[5, 6, 7, 8]).await.unwrap();
        st.write_at(4, &[2, 2, 2, 2]).await.unwrap();
        st.flush().await.unwrap();
        assert_eq!(
            st.open_count(),
            1,
            "C++ DefaultDiskWriter keeps dest fd across pwrite chunks"
        );
        assert_eq!(st.write_count(), 4);
        let mut want = vec![0u8; 16];
        want[0..4].copy_from_slice(&[1, 1, 1, 1]);
        want[4..8].copy_from_slice(&[2, 2, 2, 2]);
        want[8..12].copy_from_slice(&[5, 6, 7, 8]);
        want[12..16].copy_from_slice(&[9, 9, 9, 9]);
        assert_eq!(std::fs::read(&path).unwrap(), want);
    }

    #[tokio::test]
    async fn default_disk_writer_truncate_sparse_pwrite_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.bin");
        let st = FileStorage::with_cache(path.clone(), 12, AllocMode::Trunc, 0);
        reset_sync_trunc();
        st.ensure().await.unwrap();
        assert_eq!(last_sync_trunc(), 1);
        st.write_body(0, b"AAAA").await.unwrap();
        st.write_body(8, b"CCCC").await.unwrap();
        let got = std::fs::read(&path).unwrap();
        assert_eq!(&got[0..4], b"AAAA");
        assert_eq!(&got[4..8], &[0, 0, 0, 0]);
        assert_eq!(&got[8..12], b"CCCC");
    }

    #[tokio::test]
    async fn default_disk_writer_pread_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("r.bin");
        let st = FileStorage::with_cache(path.clone(), 12, AllocMode::None, 0);
        st.ensure().await.unwrap();
        st.write_body(0, b"HEADTAILXXXX").await.unwrap();
        reset_try_pread();
        let mut mid = [0u8; 4];
        assert!(
            st.try_pread(4, &mut mid).unwrap(),
            "C++ DefaultDiskWriter::readData pread dest fd"
        );
        assert_eq!(&mid, b"TAIL");
        let mut head = [0u8; 4];
        let n = pread_at(&path, &mut head, 0).unwrap();
        assert_eq!(n, 4);
        assert_eq!(&head, b"HEAD");
        assert!(last_try_pread() >= 2);
    }

    #[tokio::test]
    async fn file_mkdirs_nested_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("c.bin");
        let st = FileStorage::with_cache(path.clone(), 4, AllocMode::None, 0);
        reset_mkdir();
        st.ensure().await.unwrap();
        assert_eq!(
            last_mkdir(),
            1,
            "C++ File::mkdirs must create dest parents"
        );
        assert!(path.parent().unwrap().is_dir());
        st.write_body(0, b"NEST").await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"NEST");
    }

    #[test]
    fn file_write_read_unlink_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sess").join("aria2.session");
        reset_file_write();
        reset_file_read();
        reset_unlink();
        write_file(&path, b"http://example/a\n").unwrap();
        assert_eq!(last_file_write(), 1, "C++ File::write session dest");
        assert_eq!(std::fs::read(&path).unwrap(), b"http://example/a\n");
        let got = read_file(&path).unwrap();
        assert_eq!(got, b"http://example/a\n");
        assert_eq!(last_file_read(), 1, "C++ File::read input-file dest");
        assert!(unlink(&path));
        assert_eq!(last_unlink(), 1, "C++ File::remove dest");
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn file_allocation_falloc_posix_fallocate_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("falloc.bin");
        let st = FileStorage::with_cache(path.clone(), 32, AllocMode::Falloc, 0);
        reset_sync_falloc();
        st.ensure().await.unwrap();
        assert_eq!(last_sync_falloc(), 1, "C++ posix_fallocate on dest fd");
        assert_eq!(last_alloc_kind(), "falloc");
        assert!(std::fs::metadata(&path).unwrap().len() >= 32);
        st.write_body(0, b"HEAD").await.unwrap();
        st.write_body(28, b"TAIL").await.unwrap();
        let got = std::fs::read(&path).unwrap();
        assert_eq!(&got[0..4], b"HEAD");
        assert_eq!(&got[28..32], b"TAIL");
        assert_eq!(got.len(), 32);
    }

    #[tokio::test]
    async fn file_allocation_prealloc_zero_fill_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pre.bin");
        let st = FileStorage::with_cache(path.clone(), 20, AllocMode::Prealloc, 0);
        reset_sync_prealloc();
        st.ensure().await.unwrap();
        assert_eq!(last_sync_prealloc(), 1, "C++ PREALLOC 16KiB zeros on dest fd");
        assert_eq!(last_alloc_kind(), "prealloc");
        assert_eq!(std::fs::read(&path).unwrap(), vec![0u8; 20]);
        st.write_body(0, b"OK").await.unwrap();
        let got = std::fs::read(&path).unwrap();
        assert_eq!(&got[0..2], b"OK");
        assert_eq!(&got[2..], &[0u8; 18]);
    }

    #[tokio::test]
    async fn file_pool_pwrite_reuse_fd_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pool.bin");
        let pool = FilePool::new(1);
        let st = FileStorage::with_cache(path.clone(), 8, AllocMode::None, 0).with_pool(Arc::clone(&pool));
        st.ensure().await.unwrap();
        for i in 0..8u8 {
            st.write_at(i as u64, &[i.wrapping_add(0x10)]).await.unwrap();
        }
        st.flush().await.unwrap();
        let want: Vec<u8> = (0..8u8).map(|i| i.wrapping_add(0x10)).collect();
        assert_eq!(std::fs::read(&path).unwrap(), want);
    }

    #[tokio::test]
    async fn inline_pwrite_concurrent_split_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conc.bin");
        let total = 32 * 1024usize;
        let st = Arc::new(FileStorage::with_cache(
            path.clone(),
            total as u64,
            AllocMode::Trunc,
            0,
        ));
        st.ensure().await.unwrap();
        let want: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();
        let chunk = 4096usize;
        let mut joins = Vec::new();
        for i in 0..(total / chunk) {
            let st = Arc::clone(&st);
            let data = want[i * chunk..(i + 1) * chunk].to_vec();
            let off = (i * chunk) as u64;
            joins.push(tokio::spawn(async move {
                st.write_at(off, &data).await.unwrap();
            }));
        }
        for j in joins {
            j.await.unwrap();
        }
        assert_eq!(st.open_count(), 1, "one dest fd across concurrent pwrite");
        assert_eq!(st.write_count(), (total / chunk) as u64);
        assert_eq!(std::fs::read(&path).unwrap(), want);
    }

    #[tokio::test]
    async fn inline_pwrite_file_pool_concurrent_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pool-conc.bin");
        let total = 16 * 1024usize;
        let pool = FilePool::new(1);
        let st = Arc::new(
            FileStorage::with_cache(path.clone(), total as u64, AllocMode::None, 0)
                .with_pool(Arc::clone(&pool)),
        );
        st.ensure().await.unwrap();
        let want: Vec<u8> = (0..total).map(|i| (i % 199) as u8).collect();
        let chunk = 2048usize;
        let mut joins = Vec::new();
        for i in 0..(total / chunk) {
            let st = Arc::clone(&st);
            let data = want[i * chunk..(i + 1) * chunk].to_vec();
            let off = (i * chunk) as u64;
            joins.push(tokio::spawn(async move {
                st.write_at(off, &data).await.unwrap();
            }));
        }
        for j in joins {
            j.await.unwrap();
        }
        assert_eq!(std::fs::read(&path).unwrap(), want);
    }

    #[tokio::test]
    async fn default_disk_writer_lockfree_fd_concurrent_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("once.bin");
        let total = 64 * 1024usize;
        let st = Arc::new(FileStorage::with_cache(
            path.clone(),
            total as u64,
            AllocMode::Trunc,
            0,
        ));
        st.ensure().await.unwrap();
        assert!(
            st.held.get().is_some(),
            "C++ DefaultDiskWriter publishes dest fd after ensure"
        );
        let want: Vec<u8> = (0..total).map(|i| (i % 247) as u8).collect();
        let chunk = 1024usize;
        let mut joins = Vec::new();
        for i in 0..(total / chunk) {
            let st = Arc::clone(&st);
            let data = want[i * chunk..(i + 1) * chunk].to_vec();
            let off = (i * chunk) as u64;
            joins.push(tokio::spawn(async move {
                st.write_at(off, &data).await.unwrap();
            }));
        }
        for j in joins {
            j.await.unwrap();
        }
        assert_eq!(st.open_count(), 1, "one dest fd; lock-free pwrite after open");
        assert_eq!(st.write_count(), (total / chunk) as u64);
        assert_eq!(std::fs::read(&path).unwrap(), want);
    }

    #[tokio::test]
    async fn multidisk_adaptor_last_writer_same_file_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("last.bin");
        let total = 32 * 1024usize;
        let pool = FilePool::new(1);
        let st = FileStorage::with_cache(path.clone(), total as u64, AllocMode::None, 0)
            .with_pool(Arc::clone(&pool));
        st.ensure().await.unwrap();
        let want: Vec<u8> = (0..total).map(|i| (i % 211) as u8).collect();
        let chunk = 1024usize;
        let n = total / chunk;
        for i in 0..n {
            st.write_at((i * chunk) as u64, &want[i * chunk..(i + 1) * chunk])
                .await
                .unwrap();
        }
        assert_eq!(
            pool.lookups(),
            1,
            "C++ MultiDiskAdaptor: one map lookup then last DiskWriter"
        );
        assert_eq!(
            pool.last_hits(),
            (n - 1) as u64,
            "remaining writes must hit last DiskWriter"
        );
        assert_eq!(std::fs::read(&path).unwrap(), want);
    }

    #[tokio::test]
    async fn multidisk_adaptor_last_writer_pread_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seed.bin");
        let pool = FilePool::new(1);
        let st = FileStorage::with_cache(path.clone(), 16, AllocMode::None, 0)
            .with_pool(Arc::clone(&pool));
        st.ensure().await.unwrap();
        st.write_at(0, b"ABCDEFGHIJKLMNOP").await.unwrap();
        let hits0 = pool.last_hits();
        let mut a = [0u8; 4];
        assert!(st.try_pread(0, &mut a).unwrap());
        assert_eq!(&a, b"ABCD");
        let mut b = [0u8; 4];
        assert!(st.try_pread(12, &mut b).unwrap());
        assert_eq!(&b, b"MNOP");
        assert!(
            pool.last_hits() > hits0,
            "C++ MultiDiskAdaptor::readData hits last DiskWriter"
        );
        let mut mid = [0u8; 4];
        pool.read_at(&path, 4, &mut mid).unwrap();
        assert_eq!(&mid, b"EFGH");
    }

    #[tokio::test]
    async fn multidisk_adaptor_last_writer_switches_files_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        let pool = FilePool::new(2);
        let sa = FileStorage::with_cache(a.clone(), 8, AllocMode::None, 0).with_pool(Arc::clone(&pool));
        let sb = FileStorage::with_cache(b.clone(), 8, AllocMode::None, 0).with_pool(Arc::clone(&pool));
        sa.ensure().await.unwrap();
        sb.ensure().await.unwrap();
        sa.write_at(0, b"AAAA").await.unwrap();
        sb.write_at(0, b"BBBB").await.unwrap();
        reset_try_pwrite();
        sa.write_body(4, b"aaaa").await.unwrap();
        sb.write_body(4, b"bbbb").await.unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), b"AAAAaaaa");
        assert_eq!(std::fs::read(&b).unwrap(), b"BBBBbbbb");
        assert!(
            pool.map_hits() >= 2,
            "C++ MultiDiskAdaptor: returning to an open dest is a map pwrite, not last-only"
        );
        assert!(
            last_try_pwrite() >= 2,
            "write_body must pwrite via pool map without await"
        );
        assert!(
            pool.lookups() >= 2,
            "switching dest files must miss last DiskWriter"
        );
    }

    #[tokio::test]
    async fn multidisk_adaptor_lru_touch_keeps_hot_file_dest_match() {
        // C++ MultiDiskAdaptor: map-hit splices DiskWriter to LRU back; next open
        // evicts the cold file, not the just-touched one.
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("hot.bin");
        let b = dir.path().join("cold.bin");
        let c = dir.path().join("new.bin");
        let pool = FilePool::new(2);
        let sa = FileStorage::with_cache(a.clone(), 8, AllocMode::None, 0).with_pool(Arc::clone(&pool));
        let sb = FileStorage::with_cache(b.clone(), 8, AllocMode::None, 0).with_pool(Arc::clone(&pool));
        let sc = FileStorage::with_cache(c.clone(), 8, AllocMode::None, 0).with_pool(Arc::clone(&pool));
        sa.ensure().await.unwrap();
        sb.ensure().await.unwrap();
        sc.ensure().await.unwrap();
        sa.write_at(0, b"AAAA").await.unwrap();
        sb.write_at(0, b"BBBB").await.unwrap();
        sa.write_at(4, b"aaaa").await.unwrap(); // last-hit A, LRU A hot
        let hits = pool.map_hits();
        sc.write_at(0, b"CCCC").await.unwrap(); // must evict B, keep A
        sa.write_at(0, b"HOT!").await.unwrap();
        assert!(
            pool.map_hits() > hits,
            "C++ MultiDiskAdaptor LRU touch: hot dest stays in pool after third open"
        );
        assert_eq!(std::fs::read(&a).unwrap(), b"HOT!aaaa");
        assert_eq!(std::fs::read(&b).unwrap(), b"BBBB");
        assert_eq!(std::fs::read(&c).unwrap(), b"CCCC");
    }

    #[tokio::test]
    async fn multidisk_adaptor_concurrent_open_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let pool = FilePool::new(4);
        let n = 8 * 1024usize;
        let mut stores = Vec::new();
        let mut wants = Vec::new();
        let mut paths = Vec::new();
        for i in 0..4u8 {
            let path = dir.path().join(format!("f{i}.bin"));
            let st = Arc::new(
                FileStorage::with_cache(path.clone(), n as u64, AllocMode::None, 0)
                    .with_pool(Arc::clone(&pool)),
            );
            st.ensure().await.unwrap();
            let want: Vec<u8> = (0..n).map(|b| (b as u8).wrapping_add(i)).collect();
            paths.push(path);
            wants.push(want);
            stores.push(st);
        }
        reset_sync_open();
        let mut joins = Vec::new();
        for (st, want) in stores.iter().zip(wants.iter()) {
            let st = Arc::clone(st);
            let want = want.clone();
            joins.push(tokio::spawn(async move {
                st.write_at(0, &want).await.unwrap();
            }));
        }
        for j in joins {
            j.await.unwrap();
        }
        for (path, want) in paths.iter().zip(wants.iter()) {
            assert_eq!(
                std::fs::read(path).unwrap(),
                *want,
                "C++ MultiDiskAdaptor concurrent openFile dest-match {}",
                path.display()
            );
        }
        assert!(
            pool.lookups() >= 4,
            "first write of each dest must miss last DiskWriter"
        );
        assert!(
            last_sync_open() >= 4,
            "C++ MultiDiskAdaptor::openFile is POSIX open on the download thread"
        );
    }

    #[tokio::test]
    async fn multidisk_adaptor_concurrent_open_cap1_dest_match() {
        let dir = tempfile::tempdir().unwrap();
        let pool = FilePool::new(1);
        crate::storage::reset_open_peak();
        let n = 4096usize;
        let mut stores = Vec::new();
        let mut wants = Vec::new();
        let mut paths = Vec::new();
        for i in 0..3u8 {
            let path = dir.path().join(format!("c{i}.bin"));
            let st = Arc::new(
                FileStorage::with_cache(path.clone(), n as u64, AllocMode::None, 0)
                    .with_pool(Arc::clone(&pool)),
            );
            st.ensure().await.unwrap();
            let want: Vec<u8> = (0..n).map(|b| (b as u8).wrapping_add(0x30 + i)).collect();
            paths.push(path);
            wants.push(want);
            stores.push(st);
        }
        let mut joins = Vec::new();
        for (st, want) in stores.iter().zip(wants.iter()) {
            let st = Arc::clone(st);
            let want = want.clone();
            joins.push(tokio::spawn(async move {
                st.write_at(0, &want).await.unwrap();
            }));
        }
        for j in joins {
            j.await.unwrap();
        }
        for (path, want) in paths.iter().zip(wants.iter()) {
            assert_eq!(std::fs::read(path).unwrap(), *want);
        }
        assert_eq!(
            crate::storage::last_open_peak(),
            1,
            "C++ --bt-max-open-files=1 peak stays 1 across concurrent opens"
        );
    }
}
