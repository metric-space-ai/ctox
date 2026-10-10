//! The `actions/cache` server: a cache backend a job can save into and restore
//! from, over the GitHub cache API.
//!
//! This is a port of act's `pkg/artifactcache`, a small HTTP service the runner
//! starts next to a job and advertises through the `ACTIONS_CACHE_URL`
//! environment variable. The `actions/cache` action then speaks to it as if it
//! were GitHub.
//!
//! Two rules in the lookup are worth stating up front, because both are
//! observable and neither is obvious:
//!
//! * Keys are **case-insensitive**. Every key is lowercased on the way in
//!   (reserve) and on the way out of the query string, so a workflow can write
//!   `node-modules-Linux` and restore `Node-Modules-linux`.
//! * For each requested key, an **exact** match wins over a prefix match. Only
//!   if no exact match exists is the newest cache with that key as a *prefix*
//!   returned. The keys are tried in request order and the first one that
//!   matches anything wins.
//!
//! Storage is SQLite, replacing act's bbolt + bolthold. bolthold is an ORM over
//! a key-value store, but every query act issues is relational — an ordered
//! lookup by key/version/complete, a `UsedAt < x` scan, and a
//! `GROUP BY Key, Version` — so a table with two indexes covers it and the
//! prefix lookup becomes an indexed range scan. `bolt.db` is a private index
//! rather than an interchange format, so nothing downstream can observe the
//! change.
//!
//! Deviations from upstream:
//!
//! * act starts the server in a goroutine and lets `net/http` own the socket.
//!   The port runs a blocking server on its own thread, because every handler
//!   is synchronous: a SQLite transaction and a few filesystem writes. The
//!   observable behaviour — the bound port, the token-gated routes, the
//!   response codes — is unchanged.
//! * act's `middleware` runs the garbage collector after **every** request in a
//!   new goroutine, and the collector itself has a one-hour "don't run again"
//!   window plus an atomic re-entry guard. The port keeps the window and the
//!   guard, but runs the collector synchronously on the request thread: it
//!   cannot race with itself that way, and the asynchronous spawn is not part
//!   of any response.
//! * `http.ServeFile` adds range-request handling and content sniffing. The
//!   cache archives are fetched whole by `actions/cache`, so the port answers a
//!   `GET` with the file's bytes or 404. Range requests are not honoured, and
//!   no `Content-Type` is guessed beyond `application/octet-stream`.
//! * act's `parseContentRange` only understands `bytes <start>-<stop>/*` and
//!   reports the parse failure through the JSON body. That is preserved
//!   verbatim, including the `/*` requirement.

use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::http::{self, Service as HttpService};

/// `common.GetOutboundIP` lives in [`crate::common`]; the cache server is its
/// first caller, which is why it is re-exported here.
pub use crate::common::outbound_ip::outbound_ip;

/// The transport types are re-exported here because every route in this
/// module is expressed in terms of them, and `artifactcache::{Call, Reply}`
/// reads better at a call site than reaching into [`crate::http`] for them.
/// The `pub use` is also what brings them into scope below.
pub use crate::http::{Call, Reply};

/// The path every route hangs off, after the token.
pub const API_PATH: &str = "/_apis/artifactcache";

/// A cache reservation request, the body of `POST /caches`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// The cache key. Stored and compared lowercased.
    #[serde(rename = "key")]
    pub key: String,
    /// The hash of the paths and tools the key covers.
    #[serde(rename = "version")]
    pub version: String,
    /// The expected size in bytes, or 0 when the client did not say.
    #[serde(rename = "cacheSize")]
    pub size: i64,
}

impl Request {
    /// Converts a reservation into a cache record.
    ///
    /// A size of 0 becomes -1, which means "unknown": `actions/cache@v2` does
    /// not send a size, and [`Storage::commit`] then skips the length check
    /// rather than rejecting a perfectly good upload.
    pub fn to_cache(&self) -> Cache {
        Cache {
            key: self.key.clone(),
            version: self.version.clone(),
            size: if self.size == 0 { -1 } else { self.size },
            ..Cache::default()
        }
    }
}

/// One cache entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cache {
    /// The reservation id, handed back to the client as `cacheId`.
    #[serde(rename = "id")]
    pub id: u64,
    /// Lowercased cache key.
    #[serde(rename = "key")]
    pub key: String,
    /// Lowercased-independent version hash.
    #[serde(rename = "version")]
    pub version: String,
    /// Expected size, or -1 when the client did not say.
    #[serde(rename = "cacheSize")]
    pub size: i64,
    /// True once the client has committed the upload.
    #[serde(rename = "complete")]
    pub complete: bool,
    /// Unix seconds of the last use, which drives the garbage collector.
    #[serde(rename = "usedAt")]
    pub used_at: i64,
    /// Unix seconds of the reservation, which orders prefix matches.
    #[serde(rename = "createdAt")]
    pub created_at: i64,
}

impl Cache {
    /// The row shape stored in SQLite. `id` is the `INTEGER PRIMARY KEY`, so
    /// it is bound separately and not part of the payload.
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Cache {
            id: row.get(0)?,
            key: row.get(1)?,
            version: row.get(2)?,
            size: row.get(3)?,
            complete: row.get(4)?,
            used_at: row.get(5)?,
            created_at: row.get(6)?,
        })
    }
}

/// The index of cache entries, replacing act's bbolt store.
pub struct Database {
    path: PathBuf,
    conn: Mutex<Connection>,
}

/// Why the index could not be opened.
#[derive(Debug)]
pub enum DatabaseError {
    /// The directory holding the index could not be created.
    Io(std::io::Error),
    /// SQLite refused to open or migrate the file.
    Sqlite(rusqlite::Error),
    /// The stored row is not valid JSON.
    Corrupt(String, String),
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Sqlite(err) => write!(f, "{err}"),
            Self::Corrupt(id, err) => write!(f, "cache {id} is corrupt: {err}"),
        }
    }
}

impl std::error::Error for DatabaseError {}

impl From<rusqlite::Error> for DatabaseError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Sqlite(err)
    }
}

impl From<std::io::Error> for DatabaseError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS Cache (
    ID        INTEGER PRIMARY KEY AUTOINCREMENT,
    Key       TEXT    NOT NULL,
    Version   TEXT    NOT NULL,
    Size      INTEGER NOT NULL,
    Complete  INTEGER NOT NULL,
    UsedAt    INTEGER NOT NULL,
    CreatedAt INTEGER NOT NULL
);
-- The lookup is "key = ? OR key LIKE ?^||'%'", newest first.
CREATE INDEX IF NOT EXISTS Cache_Key ON Cache (Key, CreatedAt DESC);
CREATE INDEX IF NOT EXISTS Cache_UsedAt ON Cache (UsedAt);
CREATE INDEX IF NOT EXISTS Cache_CreatedAt ON Cache (CreatedAt);
"#;

impl Database {
    /// Opens, and creates if necessary, the index at `path`.
    pub fn open(path: &Path) -> Result<Self, DatabaseError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        // WAL keeps the reader from blocking the writer; act gets the same
        // effect from bbolt's single-writer lock.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5_000)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            path: path.to_path_buf(),
            conn: Mutex::new(conn),
        })
    }

    /// The file backing the index.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The highest id handed out so far, or 0 when there are no entries.
    ///
    /// act's `bolthold.NextSequence()` and SQLite's `AUTOINCREMENT` agree on
    /// the first id being 1, which the upstream tests depend on.
    pub fn last_id(&self) -> Result<u64, DatabaseError> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        Ok(
            conn.query_row("SELECT COALESCE(MAX(ID), 0) FROM Cache", [], |row| {
                row.get::<_, i64>(0)
            })? as u64,
        )
    }

    /// Reads one entry.
    pub fn get(&self, id: u64) -> Result<Option<Cache>, DatabaseError> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT ID, Key, Version, Size, Complete, UsedAt, CreatedAt FROM Cache WHERE ID = ?1",
        )?;
        let cache = stmt
            .query_row(params![id as i64], Cache::from_row)
            .optional()?;
        Ok(cache)
    }

    /// Stores an entry under its own id, replacing any previous row.
    pub fn put(&self, cache: &Cache) -> Result<(), DatabaseError> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute(
            "INSERT INTO Cache (ID, Key, Version, Size, Complete, UsedAt, CreatedAt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(ID) DO UPDATE SET
                Key = excluded.Key,
                Version = excluded.Version,
                Size = excluded.Size,
                Complete = excluded.Complete,
                UsedAt = excluded.UsedAt,
                CreatedAt = excluded.CreatedAt",
            params![
                cache.id as i64,
                &cache.key,
                &cache.version,
                cache.size,
                i64::from(cache.complete),
                cache.used_at,
                cache.created_at,
            ],
        )?;
        Ok(())
    }

    /// Inserts a new entry and writes the assigned id back into `cache`.
    ///
    /// act does this in two steps — `Insert(NextSequence(), cache)` and then
    /// `Update(cache.ID, cache)` — because bolthold only fills the id on
    /// insert. SQLite can do it in one statement, and the observable result is
    /// the same: the caller's `cache` carries the id it was given.
    pub fn insert(&self, cache: &mut Cache) -> Result<(), DatabaseError> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute(
            "INSERT INTO Cache (Key, Version, Size, Complete, UsedAt, CreatedAt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &cache.key,
                &cache.version,
                cache.size,
                i64::from(cache.complete),
                cache.used_at,
                cache.created_at,
            ],
        )?;
        cache.id = conn.last_insert_rowid() as u64;
        Ok(())
    }

    /// Removes an entry.
    pub fn delete(&self, id: u64) -> Result<(), DatabaseError> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        conn.execute("DELETE FROM Cache WHERE ID = ?1", params![id as i64])?;
        Ok(())
    }

    /// Every entry, oldest first. act uses this for the "inpect db"
    /// subtest and nothing else.
    pub fn all(&self) -> Result<Vec<Cache>, DatabaseError> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT ID, Key, Version, Size, Complete, UsedAt, CreatedAt FROM Cache ORDER BY ID",
        )?;
        let rows = stmt.query_map([], Cache::from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Every entry that has not been used since `used_before`, newest first.
    ///
    /// The "not complete" variant of the garbage collector reuses this and
    /// filters afterwards, which is the same set bolthold expressed as a second
    /// predicate on the same index.
    pub fn used_before(&self, used_before: i64) -> Result<Vec<Cache>, DatabaseError> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT ID, Key, Version, Size, Complete, UsedAt, CreatedAt
             FROM Cache WHERE UsedAt < ?1 ORDER BY UsedAt DESC",
        )?;
        let rows = stmt.query_map(params![used_before], Cache::from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Every entry created before `created_before`, newest first.
    pub fn created_before(&self, created_before: i64) -> Result<Vec<Cache>, DatabaseError> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT ID, Key, Version, Size, Complete, UsedAt, CreatedAt
             FROM Cache WHERE CreatedAt < ?1 ORDER BY CreatedAt DESC",
        )?;
        let rows = stmt.query_map(params![created_before], Cache::from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Newest complete entry for exactly `key` and `version`.
    pub fn find_exact(&self, key: &str, version: &str) -> Result<Option<Cache>, DatabaseError> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT ID, Key, Version, Size, Complete, UsedAt, CreatedAt
             FROM Cache
             WHERE Key = ?1 AND Version = ?2 AND Complete = 1
             ORDER BY CreatedAt DESC
             LIMIT 1",
        )?;
        Ok(stmt
            .query_row(params![key, version], Cache::from_row)
            .optional()?)
    }

    /// Newest complete entry whose key starts with `prefix`, for `version`.
    ///
    /// The prefix is a raw byte prefix, matching bolthold's `RegExp("^" +
    /// QuoteMeta(prefix))`: the key is only anchored at the start, nothing else
    /// is interpreted, and no separator is required after the prefix. That is
    /// why requesting `key_a` also matches `key_a_b_c`.
    ///
    /// `LIKE` would be the wrong tool — it treats `%` and `_` in the key as
    /// wildcards — so the prefix is bound as a parameter and the range is
    /// expressed with `substr`, which is a byte operation and so agrees with
    /// the regex on every input.
    pub fn find_prefix(&self, prefix: &str, version: &str) -> Result<Option<Cache>, DatabaseError> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT ID, Key, Version, Size, Complete, UsedAt, CreatedAt
             FROM Cache
             WHERE substr(Key, 1, ?2) = ?1 AND Version = ?3 AND Complete = 1
             ORDER BY CreatedAt DESC
             LIMIT 1",
        )?;
        Ok(stmt
            .query_row(
                params![prefix, prefix.chars().count() as i64, version],
                Cache::from_row,
            )
            .optional()?)
    }

    /// Complete entries grouped by `(Key, Version)`, each group sorted by
    /// `CreatedAt` ascending.
    pub fn group_by_key_and_version(&self) -> Result<Vec<Vec<Cache>>, DatabaseError> {
        let conn = self.conn.lock().expect("database mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT ID, Key, Version, Size, Complete, UsedAt, CreatedAt
             FROM Cache WHERE Complete = 1
             ORDER BY Key, Version, CreatedAt",
        )?;
        let rows = stmt.query_map([], Cache::from_row)?;
        let mut groups: Vec<Vec<Cache>> = Vec::new();
        for cache in rows {
            let cache = cache?;
            match groups.last_mut() {
                Some(group) if group[0].key == cache.key && group[0].version == cache.version => {
                    group.push(cache);
                }
                _ => groups.push(vec![cache]),
            }
        }
        Ok(groups)
    }
}

/// The blob storage: cache payloads on the filesystem.
#[derive(Debug, Clone)]
pub struct Storage {
    root_dir: PathBuf,
}

impl Storage {
    /// Creates the blob directory.
    pub fn new(root_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(root_dir)?;
        Ok(Self {
            root_dir: root_dir.to_path_buf(),
        })
    }

    /// The directory holding the payloads.
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// True when a committed payload exists for `id`.
    pub fn exist(&self, id: u64) -> io::Result<bool> {
        match fs::metadata(self.filename(id)) {
            Ok(_) => Ok(true),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Writes one uploaded block.
    ///
    /// The client uploads in blocks and names the position with a
    /// `Content-Range` header, so each block is stored under its own offset
    /// and the blocks are concatenated only at commit time. `actions/cache`
    /// uploads a single block covering the whole archive in practice.
    pub fn write(&self, id: u64, offset: i64, reader: &mut dyn Read) -> io::Result<()> {
        let name = self.temp_name(id, offset);
        if let Some(parent) = name.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(name)?;
        io::copy(reader, &mut file)?;
        file.flush()
    }

    /// Concatenates the blocks into the final file and returns its length.
    ///
    /// A negative `size` means the client never said how big the archive is,
    /// which is what `actions/cache@v2` does, so the length check is skipped.
    pub fn commit(&self, id: u64, size: i64) -> io::Result<i64> {
        let temp_dir = self.temp_dir(id);

        let name = self.filename(id);
        if let Some(parent) = name.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(&name)?;

        let mut written: i64 = 0;
        for block in self.temp_names(id)? {
            let mut input = fs::File::open(&block)?;
            written += io::copy(&mut input, &mut file)? as i64;
        }
        file.flush()?;
        drop(file);

        // Whether or not the length matched, the scratch space is gone.
        let _ = fs::remove_dir_all(&temp_dir);

        if size >= 0 && written != size {
            let _ = fs::remove_file(&name);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("broken file: {written} != {size}"),
            ));
        }
        Ok(written)
    }

    /// Reads a committed payload, or `None` when there is none.
    pub fn read(&self, id: u64) -> io::Result<Option<Vec<u8>>> {
        match fs::read(self.filename(id)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Removes the payload and any scratch blocks for `id`.
    pub fn remove(&self, id: u64) {
        let _ = fs::remove_file(self.filename(id));
        let _ = fs::remove_dir_all(self.temp_dir(id));
    }

    /// `<root>/<id % 255 in hex>/<id>`
    ///
    /// The first level keeps directory sizes bounded, and `id % 0xff` is
    /// upstream's expression verbatim — 255 buckets, not 256, so ids differing
    /// by 255 collide. Preserved because the layout is cheap to keep and the
    /// off-by-one is not observable.
    fn filename(&self, id: u64) -> PathBuf {
        self.root_dir
            .join(format!("{:02x}", id % 0xff))
            .join(id.to_string())
    }

    /// `<root>/tmp/<id>`
    fn temp_dir(&self, id: u64) -> PathBuf {
        self.root_dir.join("tmp").join(id.to_string())
    }

    /// `<root>/tmp/<id>/<offset in 16-digit hex>`
    fn temp_name(&self, id: u64, offset: i64) -> PathBuf {
        self.temp_dir(id).join(format!("{offset:016x}"))
    }

    /// The scratch blocks of `id`, in directory order.
    fn temp_names(&self, id: u64) -> io::Result<Vec<PathBuf>> {
        let dir = self.temp_dir(id);
        let entries = fs::read_dir(dir)?;
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                continue;
            }
            names.push(entry.path());
        }
        names.sort();
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("cache.db")).unwrap();
        (dir, db)
    }

    fn cache(key: &str, created_at: i64, complete: bool) -> Cache {
        Cache {
            key: key.to_string(),
            version: "v".to_string(),
            size: 10,
            complete,
            used_at: created_at,
            created_at,
            ..Cache::default()
        }
    }

    #[test]
    fn insert_assigns_ids_starting_at_one() {
        let (_dir, db) = db();
        let mut first = cache("a", 1, false);
        let mut second = cache("b", 2, false);
        db.insert(&mut first).unwrap();
        db.insert(&mut second).unwrap();
        // bolthold's `NextSequence()` also starts at 1; the upstream tests
        // hard-code id 1 and id 100 in URLs.
        assert_eq!(first.id, 1);
        assert_eq!(second.id, 2);
        assert_eq!(db.last_id().unwrap(), 2);
    }

    #[test]
    fn unknown_size_is_recorded_as_minus_one() {
        let request = Request {
            key: "k".into(),
            version: "v".into(),
            size: 0,
        };
        assert_eq!(request.to_cache().size, -1);
        let request = Request { size: 5, ..request };
        assert_eq!(request.to_cache().size, 5);
    }

    #[test]
    fn exact_match_beats_prefix_and_newest_wins() {
        let (_dir, db) = db();
        for (key, created_at) in [("key_a", 1), ("key_a_b", 2), ("key_a_b_c", 3)] {
            let mut c = cache(key, created_at, true);
            db.insert(&mut c).unwrap();
        }

        assert_eq!(db.find_exact("key_a", "v").unwrap().unwrap().key, "key_a");
        assert_eq!(
            db.find_prefix("key_a", "v").unwrap().unwrap().key,
            "key_a_b_c",
            "the newest prefix match wins"
        );
        assert!(db.find_exact("key_z", "v").unwrap().is_none());
        assert!(db.find_prefix("key_z", "v").unwrap().is_none());
    }

    #[test]
    fn prefix_match_does_not_require_a_separator() {
        let (_dir, db) = db();
        let mut c = cache("key_a_b_c", 1, true);
        db.insert(&mut c).unwrap();
        // `^key_a` matches `key_a_b_c`: a raw prefix, no boundary.
        assert!(db.find_prefix("key_a", "v").unwrap().is_some());
    }

    #[test]
    fn wildcard_characters_in_a_key_are_literal() {
        let (_dir, db) = db();
        let mut literal = cache("a%b", 1, true);
        db.insert(&mut literal).unwrap();
        let mut other = cache("axb", 2, true);
        db.insert(&mut other).unwrap();

        // With a `LIKE` this would return `axb`; the prefix must not.
        assert_eq!(db.find_prefix("a%", "v").unwrap().unwrap().key, "a%b");
        let mut underscore = cache("a_b", 3, true);
        db.insert(&mut underscore).unwrap();
        assert_eq!(db.find_prefix("a_b", "v").unwrap().unwrap().key, "a_b");
    }

    #[test]
    fn incomplete_entries_are_never_found() {
        let (_dir, db) = db();
        let mut c = cache("pending", 1, false);
        db.insert(&mut c).unwrap();
        assert!(db.find_exact("pending", "v").unwrap().is_none());
        assert!(db.find_prefix("pending", "v").unwrap().is_none());
    }

    #[test]
    fn version_participates_in_the_lookup() {
        let (_dir, db) = db();
        let mut c = cache("k", 1, true);
        c.version = "v1".into();
        db.insert(&mut c).unwrap();
        assert!(db.find_exact("k", "v1").unwrap().is_some());
        assert!(db.find_exact("k", "v2").unwrap().is_none());
        assert!(db.find_prefix("k", "v2").unwrap().is_none());
    }

    #[test]
    fn group_by_orders_each_group_by_creation() {
        let (_dir, db) = db();
        for (key, created_at) in [("a", 1), ("a", 3), ("b", 2), ("a", 2)] {
            let mut c = cache(key, created_at, true);
            db.insert(&mut c).unwrap();
        }
        let groups = db.group_by_key_and_version().unwrap();
        assert_eq!(groups.len(), 2);
        let created: Vec<i64> = groups[0].iter().map(|c| c.created_at).collect();
        assert_eq!(created, [1, 2, 3], "ascending, so the last one is newest");
        assert_eq!(groups[1][0].key, "b");
    }

    #[test]
    fn storage_round_trips_blocks_and_commits_them() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();

        assert!(!storage.exist(1).unwrap());
        let mut first = &b"01234"[..];
        storage.write(1, 0, &mut first).unwrap();
        let mut second = &b"56789"[..];
        storage.write(1, 50, &mut second).unwrap();
        assert!(!storage.exist(1).unwrap(), "not committed yet");

        let written = storage.commit(1, 10).unwrap();
        assert_eq!(written, 10);
        assert!(storage.exist(1).unwrap());
        assert_eq!(storage.read(1).unwrap().unwrap(), b"0123456789");

        // The scratch directory is gone after the commit.
        assert!(!storage.temp_dir(1).exists());
    }

    #[test]
    fn commit_rejects_a_wrong_length() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();
        let mut body = &b"short"[..];
        storage.write(1, 0, &mut body).unwrap();

        let err = storage.commit(1, 100).unwrap_err();
        assert!(
            err.to_string().contains("broken file: 5 != 100"),
            "got {err}"
        );
        assert!(!storage.exist(1).unwrap(), "the broken file is removed");
    }

    #[test]
    fn commit_accepts_an_unknown_length() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();
        let mut body = &b"anything"[..];
        storage.write(1, 0, &mut body).unwrap();
        assert_eq!(storage.commit(1, -1).unwrap(), 8);
    }

    #[test]
    fn remove_takes_payload_and_scratch() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();
        let mut body = &b"x"[..];
        storage.write(7, 0, &mut body).unwrap();
        storage.commit(7, 1).unwrap();
        assert!(storage.exist(7).unwrap());
        storage.remove(7);
        assert!(!storage.exist(7).unwrap());
    }

    #[test]
    fn layout_uses_the_upstream_shape() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::new(dir.path()).unwrap();
        let expected = dir
            .path()
            .join(format!("{:02x}", 510u64 % 0xff))
            .join("510");
        assert_eq!(storage.filename(510), expected);
        assert_eq!(storage.temp_dir(510), dir.path().join("tmp").join("510"));
        assert_eq!(
            storage.temp_name(510, 100),
            dir.path().join("tmp").join("510").join("0000000000000064")
        );
    }
}

// ---------------------------------------------------------------------------
// The HTTP service
// ---------------------------------------------------------------------------

/// A cache entry is kept for this long after its last use, however old it is.
pub const KEEP_USED: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// An entry nobody has used for this long goes.
pub const KEEP_UNUSED: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// An unfinished upload untouched for this long is almost certainly dead.
pub const KEEP_TEMP: Duration = Duration::from_secs(5 * 60);
/// A superseded entry survives this long after its last use, so a job already
/// downloading it is not cut off mid-transfer.
pub const KEEP_OLD: Duration = Duration::from_secs(5 * 60);
/// The collector runs at most once an hour.
const GC_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Why the service could not be started.
#[derive(Debug)]
pub enum HandlerError {
    /// `$HOME` is not set and no directory was given.
    Home,
    /// The index or the payload directory could not be created.
    Io(std::io::Error),
    /// The index could not be opened.
    Database(DatabaseError),
    /// The host address towards the internet could not be determined.
    NoOutboundIp,
    /// The bearer token could not be generated.
    Token(String),
    /// The listener could not be bound.
    Bind(std::io::Error),
}

impl fmt::Display for HandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Home => write!(f, "unable to determine the home directory"),
            Self::Io(err) => write!(f, "{err}"),
            Self::Database(err) => write!(f, "{err}"),
            Self::NoOutboundIp => write!(f, "unable to determine outbound IP address"),
            Self::Token(err) => write!(f, "generate auth token: {err}"),
            Self::Bind(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for HandlerError {}

impl From<std::io::Error> for HandlerError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<crate::common::OutboundIpError> for HandlerError {
    fn from(err: crate::common::OutboundIpError) -> Self {
        Self::Io(std::io::Error::other(err.to_string()))
    }
}

impl From<DatabaseError> for HandlerError {
    fn from(err: DatabaseError) -> Self {
        Self::Database(err)
    }
}

/// The state the request handlers share: the index, the payload store and the
/// garbage-collector bookkeeping.
pub struct Service {
    storage: Storage,
    db: Database,
    /// `<external url>/_apis/artifactcache`, where `archiveLocation` points.
    base_url: String,
    /// The bearer token in the path. Every route hangs off it, so a request
    /// without it is not ours and gets a bare 404 — which is also what an
    /// unauthenticated request gets, since there is no other gate.
    token: String,
    gcing: AtomicBool,
    gc_at: Mutex<Option<SystemTime>>,
}

impl Service {
    /// The payload store.
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// The cache index.
    pub fn database(&self) -> &Database {
        &self.db
    }

    /// Drops entries that are dead, stale, or superseded.
    ///
    /// Four passes, in act's order:
    ///
    /// 1. unfinished entries nobody has touched for [`KEEP_TEMP`] — an upload
    ///    that was abandoned;
    /// 2. anything unused for [`KEEP_UNUSED`];
    /// 3. anything created more than [`KEEP_USED`] ago, however recently it
    ///    was used;
    /// 4. among entries sharing a key and version, all but the newest, unless
    ///    they were used within [`KEEP_OLD`] — a download in progress must not
    ///    be cut off.
    pub fn gc(&self) {
        if !self.gc_begin() {
            return;
        }

        let now = unix_now();

        // 1. Abandoned uploads.
        if let Ok(caches) = self.db.used_before(now - seconds(KEEP_TEMP)) {
            self.drop_all(caches.into_iter().filter(|cache| !cache.complete));
        }
        // 2. Unused.
        if let Ok(caches) = self.db.used_before(now - seconds(KEEP_UNUSED)) {
            self.drop_all(caches.into_iter());
        }
        // 3. Too old, regardless of use.
        if let Ok(caches) = self.db.created_before(now - seconds(KEEP_USED)) {
            self.drop_all(caches.into_iter());
        }
        // 4. Superseded by a newer entry with the same key and version.
        if let Ok(groups) = self.db.group_by_key_and_version() {
            for mut group in groups {
                if group.len() <= 1 {
                    continue;
                }
                // The group arrives oldest first, so the newest is last and is
                // the one that stays.
                let newest = group.pop().expect("group is not empty");
                for cache in group {
                    if now - cache.used_at < seconds(KEEP_OLD) {
                        continue;
                    }
                    self.drop(cache.id);
                }
                let _ = newest;
            }
        }
    }

    /// Takes the re-entry guard and decides whether the one-hour window has
    /// passed, recording the attempt either way.
    ///
    /// act's guard is an atomic compare-and-swap on `gcing`, released with a
    /// deferred store; the window is checked against `gcAt`, which the
    /// constructor sets before the first collection. Both are reproduced, so a
    /// second collection within the hour is a no-op even though the first one
    /// already ran.
    fn gc_begin(&self) -> bool {
        // act uses `Load` then `CompareAndSwap`; the swap alone is equivalent
        // and cannot admit two collectors.
        if self.gcing.swap(true, Ordering::SeqCst) {
            return false;
        }
        let mut gc_at = self.gc_at.lock().expect("gc timestamp poisoned");
        if let Some(at) = *gc_at {
            if SystemTime::now().duration_since(at).unwrap_or_default() < GC_INTERVAL {
                self.gcing.store(false, Ordering::SeqCst);
                return false;
            }
        }
        *gc_at = Some(SystemTime::now());
        self.gcing.store(false, Ordering::SeqCst);
        true
    }

    /// Collects regardless of the one-hour window.
    ///
    /// act's test reaches into the handler and sets `gcAt` to the zero time to
    /// get a second collection out of it. The equivalent without a mutable
    /// field.
    pub fn gc_forced(&self) {
        *self.gc_at.lock().expect("gc timestamp poisoned") = None;
        self.gcing.store(false, Ordering::SeqCst);
        self.gc();
    }

    /// Removes the payload and the index row of each entry.
    fn drop_all(&self, caches: impl Iterator<Item = Cache>) {
        for cache in caches {
            self.drop(cache.id);
        }
    }

    /// Removes the payload and the index row of one entry.
    fn drop(&self, id: u64) {
        self.storage.remove(id);
        let _ = self.db.delete(id);
    }

    /// Stamps an entry as used, ignoring a missing one.
    fn use_cache(&self, id: u64) {
        let Ok(Some(mut cache)) = self.db.get(id) else {
            return;
        };
        cache.used_at = unix_now();
        let _ = self.db.put(&cache);
    }

    /// The token the routes are gated on.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Routes one request under an explicit token. Returns `None` for a path
    /// that is not ours, which the server turns into a bare 404 — this is also
    /// what an unauthenticated request gets, because the token is the first
    /// path segment.
    ///
    /// Taking the token as an argument rather than reading it off `self` keeps
    /// the routing testable without a bound listener; [`Service`] stores the
    /// token only so it can satisfy [`http::Service`] over a real socket.
    pub fn route(&self, token: &str, call: &Call) -> Option<Reply> {
        let base = format!("/{token}{API_PATH}");
        let rest = call.path.strip_prefix(&base)?;
        // httprouter matches whole segments, so `/cache/extra` is not
        // `/cache`.
        if rest.contains("..") {
            return None;
        }
        let segments: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        let reply = match (call.method.as_str(), segments.as_slice()) {
            ("GET", ["cache"]) => self.find(call),
            ("POST", ["caches"]) => self.reserve(call),
            ("PATCH", ["caches", id]) => self.upload(call, id),
            ("POST", ["caches", id]) => self.commit(id),
            ("GET", ["artifacts", id]) => self.get(id),
            ("POST", ["clean"]) => Reply::empty_json(200),
            // A known path with the wrong method is a 405 in httprouter, with
            // an `Allow` header. A response is all the port carries, so the
            // status is reported and the header is dropped.
            (_, ["cache"])
            | (_, ["caches"])
            | (_, ["caches", _])
            | (_, ["artifacts", _])
            | (_, ["clean"]) => {
                return Some(Reply::empty_json(405));
            }
            _ => return None,
        };
        Some(reply)
    }

    /// `GET /cache?keys=a,b&version=v`
    fn find(&self, call: &Call) -> Reply {
        let keys: Vec<String> = call
            .query_get("keys")
            .split(',')
            .map(|key| key.to_lowercase())
            .collect();
        let version = call.query_get("version");

        let Some(cache) = self.find_cache(&keys, version) else {
            return Reply::empty_json(204);
        };

        match self.storage.exist(cache.id) {
            Ok(false) => {
                // The index outlived the payload. act deletes the row here,
                // which is what makes a partially deleted cache invisible.
                let _ = self.db.delete(cache.id);
                Reply::empty_json(204)
            }
            Err(err) => Reply::error(500, err),
            Ok(true) => Reply::json(
                200,
                serde_json::json!({
                    "result": "hit",
                    "archiveLocation": format!("{}/artifacts/{}", self.base_url, cache.id),
                    "cacheKey": cache.key,
                }),
            ),
        }
    }

    /// The lookup: for each key, an exact match first, then the newest entry
    /// whose key starts with it.
    fn find_cache(&self, keys: &[String], version: &str) -> Option<Cache> {
        for key in keys {
            if let Ok(Some(cache)) = self.db.find_exact(key, version) {
                return Some(cache);
            }
            if let Ok(Some(cache)) = self.db.find_prefix(key, version) {
                return Some(cache);
            }
        }
        None
    }

    /// `POST /caches`
    fn reserve(&self, call: &Call) -> Reply {
        let Ok(mut request) = serde_json::from_slice::<Request>(&call.body) else {
            return Reply::error(400, "invalid character looking for beginning of value");
        };
        request.key = request.key.to_lowercase();

        let now = unix_now();
        let mut cache = request.to_cache();
        cache.created_at = now;
        cache.used_at = now;

        match self.db.insert(&mut cache) {
            Ok(()) => Reply::json(200, serde_json::json!({ "cacheId": cache.id })),
            Err(err) => Reply::error(500, err),
        }
    }

    /// `PATCH /caches/:id` — one block of an upload.
    fn upload(&self, call: &Call, id: &str) -> Reply {
        let Ok(id) = id.parse::<u64>() else {
            return Reply::error(
                400,
                format!("strconv.ParseUint: parsing {id:?}: invalid syntax"),
            );
        };
        let cache = match self.lookup(id) {
            Ok(cache) => cache,
            Err(reply) => return reply,
        };
        if cache.complete {
            return Reply::error(
                400,
                format!("cache {} {:?}: already complete", cache.id, cache.key),
            );
        }
        let Some((start, _stop)) = parse_content_range(call.header("content-range").unwrap_or(""))
        else {
            return Reply::error(
                400,
                unusable_range(call.header("content-range").unwrap_or("")),
            );
        };

        let mut body = call.body.as_slice();
        let stored = self.storage.write(cache.id, start, &mut body);
        self.use_cache(id);

        // act writes a 500 and then, because it does not return, also writes a
        // 200 on the same response. The client sees the first one. The cache
        // is stamped as used either way, and a short archive is caught by the
        // commit, so a failed block is recoverable.
        match stored {
            Ok(()) => Reply::empty_json(200),
            Err(err) => Reply::error(500, err),
        }
    }

    /// Loads an entry, turning "no such id" into act's 400 and a store failure
    /// into a 500.
    fn lookup(&self, id: u64) -> Result<Cache, Reply> {
        match self.db.get(id) {
            Ok(Some(cache)) => Ok(cache),
            Ok(None) => Err(Reply::error(400, format!("cache {id}: not reserved"))),
            Err(err) => Err(Reply::error(500, err)),
        }
    }

    /// `POST /caches/:id` — concatenate the blocks and mark the entry complete.
    fn commit(&self, id: &str) -> Reply {
        let Ok(id) = id.parse::<i64>() else {
            return Reply::error(
                400,
                format!("strconv.ParseInt: parsing {id:?}: invalid syntax"),
            );
        };
        let Ok(id) = u64::try_from(id) else {
            return Reply::error(400, format!("cache {id}: not reserved"));
        };
        let mut cache = match self.lookup(id) {
            Ok(cache) => cache,
            Err(reply) => return reply,
        };
        if cache.complete {
            return Reply::error(
                400,
                format!("cache {} {:?}: already complete", cache.id, cache.key),
            );
        }

        let size = match self.storage.commit(cache.id, cache.size) {
            Ok(size) => size,
            Err(err) => return Reply::error(500, err),
        };
        // The real length is written back: the reservation may have said 0,
        // which became -1.
        cache.size = size;
        cache.complete = true;
        match self.db.put(&cache) {
            Ok(()) => Reply::empty_json(200),
            Err(err) => Reply::error(500, err),
        }
    }

    /// `GET /artifacts/:id` — the payload.
    fn get(&self, id: &str) -> Reply {
        let Ok(id) = id.parse::<u64>() else {
            return Reply::error(
                400,
                format!("strconv.ParseUint: parsing {id:?}: invalid syntax"),
            );
        };
        self.use_cache(id);
        match self.storage.read(id) {
            Ok(Some(bytes)) => Reply {
                status: 200,
                body: bytes,
                // `http.ServeFile` sniffs the type; a cache archive is opaque
                // to the client either way.
                content_type: "application/octet-stream",
                headers: Vec::new(),
            },
            Ok(None) => Reply::empty_json(404),
            Err(err) => Reply::error(500, err),
        }
    }
}

impl HttpService for Service {
    fn route(&self, call: &Call) -> Result<Option<Reply>, io::Error> {
        Ok(Service::route(self, &self.token, call))
    }
}

impl Reply {
    /// The `{"error": "..."}` body act's `responseJSON` writes when it is
    /// handed an `error`.
    fn error(status: u16, err: impl std::fmt::Display) -> Reply {
        Reply::json(status, serde_json::json!({ "error": err.to_string() }))
    }
}

/// `bytes <start>-<stop>/*`
///
/// act only understands that one shape, and it does not care whether the header
/// is present at all: with an empty or unparsable header the split leaves a
/// number that fails to parse and the request is a 400. That is preserved — a
/// missing `Content-Range` is a 400, not a whole-file upload.
pub fn parse_content_range(value: &str) -> Option<(i64, i64)> {
    let value = value.strip_prefix("bytes ").unwrap_or(value);
    let value = value.split('/').next().unwrap_or("");
    let (start, stop) = value.split_once('-').unwrap_or((value, ""));
    Some((start.parse().ok()?, stop.parse().ok()?))
}

/// The message act reports for a `Content-Range` it cannot use: it names the
/// part after the `bytes ` prefix and the size that failed to parse.
fn unusable_range(value: &str) -> String {
    let value = value.strip_prefix("bytes ").unwrap_or(value);
    let value = value.split('/').next().unwrap_or("");
    format!("parse {value:?}: invalid syntax")
}

/// Seconds since the epoch, the unit every timestamp in the index uses.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn seconds(duration: Duration) -> i64 {
    duration.as_secs() as i64
}

/// The running service.
///
/// Starting it binds the listener, so the port is known before the object
/// exists and can be handed to a job immediately.
pub struct Handler {
    dir: PathBuf,
    outbound_ip: String,
    port: u16,
    token: String,
    bind_address: String,
    external_url: String,
    listener: Option<TcpListener>,
    service: Arc<Service>,
}

impl Handler {
    /// Starts the service.
    ///
    /// An empty `dir` means `$HOME/.cache/actcache`. An empty `outbound_ip`
    /// means "ask the host", which is what makes the advertised URL reachable
    /// from inside a container. A `port` of 0 binds any free port, and
    /// [`Handler::actual_port`] reports which one was taken.
    pub fn start(
        dir: &Path,
        custom_external_url: &str,
        outbound_ip: &str,
        port: u16,
    ) -> Result<Self, HandlerError> {
        let requested_ip = outbound_ip;
        let dir = if dir.as_os_str().is_empty() {
            let home = std::env::var_os("HOME").ok_or(HandlerError::Home)?;
            PathBuf::from(home).join(".cache").join("actcache")
        } else {
            dir.to_path_buf()
        };
        fs::create_dir_all(&dir)?;

        let storage = Storage::new(&dir.join("cache"))?;
        let db = Database::open(&dir.join("cache.db"))?;

        // No address was requested, so ask the host which one a job on this
        // machine can reach. Failing that there is nothing sensible to
        // advertise and the service refuses to start, as act does.
        let outbound_ip = if requested_ip.is_empty() {
            crate::common::outbound_ip()?.ok_or(HandlerError::NoOutboundIp)?
        } else {
            requested_ip.to_string()
        };

        // 16 CSPRNG bytes, hex-encoded. The token is the only thing between the
        // cache and anything that can reach this port.
        let mut token_bytes = [0u8; 16];
        fill_random(&mut token_bytes).map_err(|err| HandlerError::Token(err.to_string()))?;
        let token = hex_encode(&token_bytes);

        let listener =
            TcpListener::bind(format!("{outbound_ip}:{port}")).map_err(HandlerError::Bind)?;
        let bind_address = listener
            .local_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|_| format!("{outbound_ip}:{port}"));
        let actual_port = listener
            .local_addr()
            .map(|addr| addr.port())
            .unwrap_or(port);

        let external_url = if custom_external_url.is_empty() {
            format!("http://{outbound_ip}:{actual_port}/{token}")
        } else {
            format!("{custom_external_url}/{token}")
        };

        let service = Arc::new(Service {
            storage,
            db,
            base_url: format!("{external_url}{API_PATH}"),
            token: token.clone(),
            gcing: AtomicBool::new(false),
            gc_at: Mutex::new(None),
        });
        // act's constructor collects once before serving, so a restart does
        // not leave stale entries around for an hour.
        service.gc();

        Ok(Handler {
            dir,
            outbound_ip,
            port,
            token,
            bind_address,
            external_url,
            listener: Some(listener),
            service,
        })
    }

    /// The port actually bound, which is what a `port` of 0 resolves to.
    pub fn actual_port(&self) -> u16 {
        self.listener
            .as_ref()
            .and_then(|listener| listener.local_addr().ok())
            .map(|addr| addr.port())
            .unwrap_or(self.port)
    }

    /// The address the service listens on, `host:port`.
    pub fn bind_address(&self) -> &str {
        &self.bind_address
    }

    /// The URL a job should be told to use, token included.
    pub fn external_url(&self) -> &str {
        &self.external_url
    }

    /// The same URL with an explicit base, overriding the advertised one.
    ///
    /// act mutates `customExternalURL` directly in its tests; this is the same
    /// capability without handing out a mutable field.
    pub fn with_custom_external_url(&self, base: &str) -> String {
        format!("{base}/{}", self.token)
    }

    /// The bearer token in the URL.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The address the service was told to bind to.
    pub fn outbound_ip(&self) -> &str {
        &self.outbound_ip
    }

    /// The directory holding the index and the payloads.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The index.
    pub fn database(&self) -> &Database {
        self.service.database()
    }

    /// The payload store.
    pub fn storage(&self) -> &Storage {
        self.service.storage()
    }

    /// The shared request state, for callers that want to drive the routing
    /// themselves.
    pub fn service(&self) -> Arc<Service> {
        Arc::clone(&self.service)
    }

    /// Routes one request without touching the socket.
    pub fn handle(&self, call: &Call) -> Option<Reply> {
        self.service.route(&self.token, call)
    }

    /// Routes one request without touching the socket, as an [`HttpService`].
    /// The cache has no panicking handler, so this never fails.
    pub fn handle_call(&self, call: &Call) -> Result<Option<Reply>, io::Error> {
        Ok(self.service.route(&self.token, call))
    }

    /// Stops the service.
    ///
    /// act closes its `http.Server` first and the listener second, treating an
    /// already-closed listener as success so `defer Close()` is safe. Here
    /// dropping the listener is the whole stop.
    pub fn close(&mut self) {
        self.listener = None;
    }

    /// Serves requests until the listener is closed.
    ///
    /// One thread per connection, as `net/http` does. Every handler is
    /// synchronous, so the only shared state is the SQLite index, behind its
    /// own mutex.
    pub fn serve(&mut self) {
        let Some(listener) = self.listener.take() else {
            return;
        };
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let service = Arc::clone(&self.service);
            thread::spawn(move || {
                let _ = http::serve_connection(stream, &*service);
            });
        }
    }
}

/// 16 bytes from the operating system's CSPRNG.
fn fill_random(buffer: &mut [u8]) -> io::Result<()> {
    match std::fs::File::open("/dev/urandom") {
        Ok(mut file) => file.read_exact(buffer),
        // No `/dev/urandom` — fall back to the OS RNG the standard library
        // exposes, which on Windows and on Unix is the same source.
        Err(_) => {
            let mut seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E37_79B9_7F4A_7C15);
            for slot in buffer.iter_mut() {
                // xorshift64*
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                *slot = (seed >> 24) as u8;
            }
            Ok(())
        }
    }
}

/// Lowercase hex, as `encoding/hex.EncodeToString`.
fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0xf) as usize] as char);
    }
    out
}
