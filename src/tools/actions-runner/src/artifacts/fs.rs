//! The filesystem act's artifact routes are given, and the two shapes of it.
//!
//! act splits the interface: `WriteFS` for creating and appending, and a plain
//! `io/fs.FS` for reading. In `Serve` both are `readWriteFSImpl`, which is
//! nothing but `os.Open`, `os.OpenFile` and `os.MkdirAll` over host paths.
//!
//! The upstream tests do **not** exercise that. They hand the routes an
//! `fstest.MapFS`, whose names are slash-separated relative paths inside a
//! map, and a `writeMapFS` wrapper whose `OpenAppendable` is a copy of
//! `OpenWritable` — it replaces the contents instead of appending. Both are
//! ported here as [`MapFs`] so the upstream assertions hold verbatim, and both
//! departures from production are pinned by a second test in production shape
//! against [`OsFs`]. A double that only resembles the real thing is not a test
//! of the real thing.

use std::collections::BTreeMap;
use std::fs as stdfs;
use std::io::{Seek, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// One entry of a directory listing, as much of it as act reads.
///
/// act uses the name for the artifact id and the filter, and calls `Info()` for
/// the size and the modification time. `Info` can fail, and then act keeps the
/// `timestamppb.Now()` and zero size it already had, so the modification time
/// is optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInfo {
    /// The entry's own name, without any directory part.
    pub name: String,
    /// Whether it is a directory, which `ListArtifacts` never filters on and
    /// the download walk always skips.
    pub is_dir: bool,
    /// The size in bytes.
    pub size: u64,
    /// The modification time, when the filesystem reports one.
    pub modified: Option<SystemTime>,
}

/// The reading and writing act does, and nothing more.
///
/// act's own interface hands out an `io.WriteCloser` and copies a whole
/// request body into it. Since every write is "open, copy the body, close" and
/// nothing is ever written in blocks, `create` and `append` express that
/// exactly and without a lifetime.
pub trait ArtifactFs: Send + Sync {
    /// `fs.Open`, read whole.
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>>;

    /// `fs.ReadDir`: the entries of one directory, sorted by name.
    fn read_dir(&self, path: &str) -> std::io::Result<Vec<EntryInfo>>;

    /// `fs.WalkDir`: every entry beneath `root`, directories included, in
    /// lexical order, each with its full path. `root` itself is the first
    /// entry, which is why a walk rooted at a file yields that one file.
    fn walk(&self, root: &str) -> std::io::Result<Vec<(String, bool)>>;

    /// `OpenWritable`: create or truncate, leaving an empty file.
    fn create(&self, path: &str) -> std::io::Result<()>;

    /// `OpenAppendable` plus the body: create if needed, seek to the end,
    /// append.
    fn append(&self, path: &str, data: &[u8]) -> std::io::Result<()>;

    /// `os.RemoveAll`. act hardcodes `os` for this one and does not go
    /// through the filesystem it was handed, so the port does not either.
    fn remove_all(&self, path: &str) -> std::io::Result<()>;
}

/// `readWriteFSImpl`: the real thing, over host paths.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsFs;

impl ArtifactFs for OsFs {
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn read_dir(&self, path: &str) -> std::io::Result<Vec<EntryInfo>> {
        let mut entries: Vec<EntryInfo> = Vec::new();
        // `std::fs::read_dir` does not sort; Go's `os.ReadDir` does, and the
        // order is visible in `ListArtifacts` and in the V3 listing.
        for entry in stdfs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata().ok();
            entries.push(EntryInfo {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: entry.file_type().map(|t| t.is_dir()).unwrap_or(false),
                size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                modified: metadata
                    .as_ref()
                    .and_then(|m| m.modified().ok()),
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    fn walk(&self, root: &str) -> std::io::Result<Vec<(String, bool)>> {
        let metadata = stdfs::metadata(root)?;
        let mut out = vec![(root.to_string(), metadata.is_dir())];
        if !metadata.is_dir() {
            return Ok(out);
        }
        // `fs.WalkDir` sorts each directory and descends depth-first, with a
        // directory's children following it.
        let mut stack = vec![root.to_string()];
        while let Some(dir) = stack.pop() {
            let mut children: Vec<String> = Vec::new();
            for entry in stdfs::read_dir(&dir)? {
                let entry = entry?;
                children.push(entry.path().to_string_lossy().into_owned());
            }
            children.sort();
            // Reversed, because the stack pops the last child first, and Go
            // visits children in ascending order.
            for child in children.into_iter().rev() {
                let is_dir = stdfs::metadata(&child).map(|m| m.is_dir()).unwrap_or(false);
                out.push((child.clone(), is_dir));
                if is_dir {
                    stack.push(child);
                }
            }
        }
        Ok(out)
    }

    fn create(&self, path: &str) -> std::io::Result<()> {
        if let Some(parent) = Path::new(path).parent() {
            stdfs::create_dir_all(parent)?;
        }
        stdfs::File::create(path)?;
        Ok(())
    }

    fn append(&self, path: &str, data: &[u8]) -> std::io::Result<()> {
        if let Some(parent) = Path::new(path).parent() {
            stdfs::create_dir_all(parent)?;
        }
        let mut file = stdfs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            // `O_APPEND` semantics without `O_APPEND`: act opens without it
            // and seeks, so a failed seek would still leave the cursor where
            // it was. Stating `truncate(false)` says the same thing.
            .truncate(false)
            .open(path)?;
        file.seek(std::io::SeekFrom::End(0))?;
        file.write_all(data)
    }

    fn remove_all(&self, path: &str) -> std::io::Result<()> {
        match stdfs::remove_dir_all(path) {
            Ok(()) => Ok(()),
            // `os.RemoveAll` returns nil when the target is already gone.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }
}

/// One `fstest.MapFile`.
#[derive(Debug, Clone)]
struct MapFile {
    data: Vec<u8>,
    modified: SystemTime,
}

/// A port of the test double: `fstest.MapFS` wrapped in `writeMapFS`.
///
/// Three things about it are *not* production, and all three are preserved
/// because the upstream assertions are written against them:
///
/// * names are `/`-separated and relative to the map, never host paths;
/// * directories are synthesised from the names, so `a/b` implies `a`;
/// * `OpenAppendable` replaces the contents, exactly like `OpenWritable`, so
///   the double cannot distinguish the two.
///
/// act passes the double **by value** — `uploads(router, base, writeMapFS{memfs})`
/// — and the two methods mutate the map through that value receiver, so the
/// routes share one store. Hence the mutex rather than `&mut self`.
#[derive(Debug, Default)]
pub struct MapFs {
    files: Mutex<BTreeMap<String, MapFile>>,
}

impl MapFs {
    /// An empty map.
    pub fn new() -> Self {
        MapFs::default()
    }

    /// `fstest.MapFS{...}`, with the upstream tests' default `ModTime`.
    ///
    /// A `MapFile` with no `ModTime` set carries Go's zero time. Storing the
    /// epoch instead keeps the type simple, and nothing upstream asserts on
    /// it: the timestamp is only read by `ListArtifacts`, whose test checks
    /// the count, the name and the URL.
    pub fn insert(&self, path: &str, data: &[u8]) {
        self.files.lock().expect("map fs poisoned").insert(
            path.trim_start_matches('/').to_string(),
            MapFile {
                data: data.to_vec(),
                modified: UNIX_EPOCH,
            },
        );
    }

    /// The contents of one file, for assertions.
    pub fn get(&self, path: &str) -> Option<Vec<u8>> {
        self.files
            .lock()
            .expect("map fs poisoned")
            .get(path.trim_start_matches('/'))
            .map(|file| file.data.clone())
    }

    /// `fs.WalkDir`'s check: the path exists, as a file or a synthesised
    /// directory.
    fn is_dir(&self, path: &str) -> bool {
        let files = self.files.lock().expect("map fs poisoned");
        let prefix = path.trim_start_matches('/');
        let prefix = if prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix}/")
        };
        files.keys().any(|key| key.starts_with(&prefix))
    }

    fn is_file(&self, path: &str) -> bool {
        self.files
            .lock()
            .expect("map fs poisoned")
            .contains_key(path.trim_start_matches('/'))
    }
}

impl ArtifactFs for MapFs {
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.files
            .lock()
            .expect("map fs poisoned")
            .get(path.trim_start_matches('/'))
            .map(|file| file.data.clone())
            .ok_or_else(|| not_found(path))
    }

    fn read_dir(&self, path: &str) -> std::io::Result<Vec<EntryInfo>> {
        let path = path.trim_start_matches('/');
        if self.is_file(path) {
            // A `MapFS` opens a regular file as a one-entry directory.
            return Ok(vec![EntryInfo {
                name: path.rsplit('/').next().unwrap_or(path).to_string(),
                is_dir: false,
                size: 0,
                modified: None,
            }]);
        }
        if !self.is_dir(path) {
            return Err(not_found(path));
        }
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}/")
        };
        // `MapFS.ReadDir` lists the immediate children only, sorted.
        let files = self.files.lock().expect("map fs poisoned");
        let mut names: Vec<String> = Vec::new();
        for key in files.keys() {
            let Some(rest) = key.strip_prefix(&prefix) else {
                continue;
            };
            let name = rest.split('/').next().unwrap_or(rest);
            if !name.is_empty() && !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
        names.sort();
        drop(files);
        Ok(names
            .into_iter()
            .map(|name| EntryInfo {
                is_dir: self.is_dir(&format!("{prefix}{name}")),
                size: 0,
                modified: None,
                name,
            })
            .collect())
    }

    fn walk(&self, root: &str) -> std::io::Result<Vec<(String, bool)>> {
        let root = root.trim_start_matches('/');
        if self.is_file(root) {
            return Ok(vec![(root.to_string(), false)]);
        }
        if !self.is_dir(root) {
            return Err(not_found(root));
        }
        // `BTreeMap` iterates in sorted order, which is the order
        // `WalkDir` produces: every entry before its own children.
        let prefix = format!("{root}/");
        let files = self.files.lock().expect("map fs poisoned");
        let mut out = vec![(root.to_string(), true)];
        for key in files.keys() {
            if key.starts_with(&prefix) {
                out.push((key.clone(), false));
            }
        }
        Ok(out)
    }

    fn create(&self, path: &str) -> std::io::Result<()> {
        self.append(path, &[])
    }

    fn append(&self, path: &str, data: &[u8]) -> std::io::Result<()> {
        let path = path.trim_start_matches('/').to_string();
        let mut files = self.files.lock().expect("map fs poisoned");
        let existing = files.get(&path).map(|file| file.modified);
        files.insert(
            path,
            MapFile {
                // `writableMapFile.Write` assigns: the body replaces whatever
                // was there, on the append path as well as the truncate one.
                data: data.to_vec(),
                modified: existing.unwrap_or(UNIX_EPOCH),
            },
        );
        Ok(())
    }

    fn remove_all(&self, path: &str) -> std::io::Result<()> {
        let path = path.trim_start_matches('/');
        let prefix = format!("{path}/");
        self.files
            .lock()
            .expect("map fs poisoned")
            .retain(|key, _| key != path && !key.starts_with(&prefix));
        Ok(())
    }
}

/// The error Go's `fs` package reports for a missing file, which the handlers
/// only ever turn into a closed connection.
fn not_found(path: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("open {path}: no such file or directory"),
    )
}
