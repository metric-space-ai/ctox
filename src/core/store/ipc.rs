//! Same-user daemon store jobs over private LocalTransport endpoints.
//! No SQL, file path, bearer credential or claimed user travels in a request.
//! CLI callers use `request` and never open SQLite. A host dispatcher maps named
//! operations to its writer/read-pool handles. No nested runtime or block_on.
use crate::{Cancellation, DbId, JobHandle, JobOptions, StoreError};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

pub const POOL_SIZE: usize = 64;
pub const MAX_FRAME: usize = 1024 * 1024;
pub const MAX_LIFETIME: Duration = Duration::from_secs(30);
const TICK: Duration = Duration::from_millis(2);
static CLIENTS: AtomicUsize = AtomicUsize::new(0);

/// Private variants of the host's LocalTransport. The Binary-owned legacy
/// transport cannot be imported into this independent crate. Host adapters map
/// its UnixSocket/NamedPipe fields here; TCP is deliberately unrepresentable.
#[derive(Clone, Debug)]
pub enum StoreEndpoint {
    UnixSocket { path: PathBuf },
    NamedPipe { name: String, directory: PathBuf },
}
impl StoreEndpoint {
    fn directory(&self) -> Result<&Path, StoreError> {
        match self {
            Self::UnixSocket { path } => path
                .parent()
                .ok_or_else(|| invalid("socket needs a directory")),
            Self::NamedPipe { directory, .. } => Ok(directory),
        }
    }
    fn lock_path(&self) -> Result<PathBuf, StoreError> {
        Ok(self.directory()?.join("store-daemon.lock"))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Read,
    Write,
}
#[derive(Clone, Debug)]
pub struct Request {
    /// Correlation only. Write handlers must deduplicate their durable op_id
    /// from the typed payload; transport request IDs are not execution receipts.
    pub request_id: u64,
    pub kind: JobKind,
    pub method: String,
    pub payload: Vec<u8>,
}
impl Request {
    fn validate(&self) -> Result<(), StoreError> {
        if self.request_id == 0
            || self.method.is_empty()
            || self.method.len() > 128
            || !self
                .method
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            || self.payload.len() > MAX_FRAME
        {
            return Err(invalid("invalid store request"));
        }
        Ok(())
    }
}
/// Must enqueue immediately, never open a file or wait on another queue here.
/// Each named method validates its payload and its Read/Write kind. No generic
/// SQL method is supplied. Runs on the IPC pool, not an async runtime worker.
pub trait Dispatcher: Send + Sync + 'static {
    fn dispatch(
        &self,
        request: Request,
        options: JobOptions,
    ) -> Result<JobHandle<Vec<u8>>, StoreError>;
}

fn invalid(message: &str) -> StoreError {
    StoreError::Invalid(message.into())
}
fn check(options: &JobOptions, stop: &AtomicBool) -> Result<(), StoreError> {
    if stop.load(Ordering::Acquire) {
        return Err(StoreError::Closed);
    }
    options.check()
}
fn options_with_limit(mut options: JobOptions) -> JobOptions {
    let cap = Instant::now() + MAX_LIFETIME;
    options.deadline = Some(options.deadline.map_or(cap, |d| d.min(cap)));
    options
}
fn pause(options: &JobOptions, stop: &AtomicBool) -> Result<(), StoreError> {
    check(options, stop)?;
    let remaining = options
        .deadline
        .unwrap()
        .saturating_duration_since(Instant::now());
    thread::park_timeout(TICK.min(remaining));
    check(options, stop)
}
trait Channel: Read + Write + Send {
    fn authenticate(&self, server: bool) -> io::Result<()>;
}
fn read_exact(
    channel: &mut dyn Channel,
    mut bytes: &mut [u8],
    options: &JobOptions,
    stop: &AtomicBool,
) -> Result<(), StoreError> {
    while !bytes.is_empty() {
        check(options, stop)?;
        match channel.read(bytes) {
            Ok(0) => return Err(StoreError::Closed),
            Ok(n) => bytes = &mut bytes[n..],
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => pause(options, stop)?,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn write_all(
    channel: &mut dyn Channel,
    mut bytes: &[u8],
    options: &JobOptions,
    stop: &AtomicBool,
) -> Result<(), StoreError> {
    while !bytes.is_empty() {
        check(options, stop)?;
        match channel.write(bytes) {
            Ok(0) => return Err(StoreError::Closed),
            Ok(n) => bytes = &bytes[n..],
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => pause(options, stop)?,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn request_bytes(request: &Request, timeout: Duration) -> Vec<u8> {
    let mut frame = Vec::with_capacity(28 + request.method.len() + request.payload.len());
    frame.extend_from_slice(b"CST4");
    frame.push(match request.kind {
        JobKind::Read => 0,
        JobKind::Write => 1,
    });
    frame.extend_from_slice(&[0; 3]);
    frame.extend_from_slice(&request.request_id.to_le_bytes());
    frame.extend_from_slice(&(timeout.as_millis().min(30_000) as u32).to_le_bytes());
    frame.extend_from_slice(&(request.method.len() as u32).to_le_bytes());
    frame.extend_from_slice(&(request.payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(request.method.as_bytes());
    frame.extend_from_slice(&request.payload);
    frame
}
fn read_request(
    channel: &mut dyn Channel,
    options: &mut JobOptions,
    stop: &AtomicBool,
) -> Result<Request, StoreError> {
    let mut header = [0u8; 28];
    read_exact(channel, &mut header, options, stop)?;
    if &header[..4] != b"CST4" || header[5..8] != [0; 3] {
        return Err(invalid("incompatible store framing"));
    }
    let kind = match header[4] {
        0 => JobKind::Read,
        1 => JobKind::Write,
        _ => return Err(invalid("invalid job kind")),
    };
    let id = u64::from_le_bytes(header[8..16].try_into().unwrap());
    let timeout = u32::from_le_bytes(header[16..20].try_into().unwrap());
    let method_len = u32::from_le_bytes(header[20..24].try_into().unwrap()) as usize;
    let body_len = u32::from_le_bytes(header[24..28].try_into().unwrap()) as usize;
    if method_len == 0 || method_len > 128 || body_len > MAX_FRAME || timeout == 0 {
        return Err(invalid("invalid store frame bounds"));
    }
    options.deadline = Some(
        options
            .deadline
            .unwrap()
            .min(Instant::now() + Duration::from_millis(timeout.min(30_000) as u64)),
    );
    let mut method = vec![0; method_len];
    read_exact(channel, &mut method, options, stop)?;
    let method = String::from_utf8(method).map_err(|_| invalid("invalid method encoding"))?;
    let mut payload = vec![0; body_len];
    read_exact(channel, &mut payload, options, stop)?;
    let request = Request {
        request_id: id,
        kind,
        method,
        payload,
    };
    request.validate()?;
    Ok(request)
}
fn error_code(error: &StoreError) -> u8 {
    match error {
        StoreError::Busy => 1,
        StoreError::Cancelled => 2,
        StoreError::DeadlineExceeded => 3,
        StoreError::Closed => 4,
        StoreError::Invalid(_) => 5,
        StoreError::Panicked => 7,
        StoreError::AlreadyOwned => 8,
        _ => 6,
    }
}
fn code_error(code: u8) -> StoreError {
    match code {
        1 => StoreError::Busy,
        2 => StoreError::Cancelled,
        3 => StoreError::DeadlineExceeded,
        4 => StoreError::Closed,
        5 => invalid("daemon rejected store request"),
        7 => StoreError::Panicked,
        8 => StoreError::AlreadyOwned,
        _ => StoreError::Io("daemon store job failed".into()),
    }
}
fn respond(
    channel: &mut dyn Channel,
    id: u64,
    result: Result<Vec<u8>, StoreError>,
    options: &JobOptions,
    stop: &AtomicBool,
) -> Result<(), StoreError> {
    let (code, body) = match result {
        Ok(body) if body.len() <= MAX_FRAME => (0, body),
        Ok(_) => (5, Vec::new()),
        Err(error) => (error_code(&error), Vec::new()),
    };
    let mut frame = Vec::with_capacity(20 + body.len());
    frame.extend_from_slice(b"CSR4");
    frame.extend_from_slice(&id.to_le_bytes());
    frame.push(code);
    frame.extend_from_slice(&[0; 3]);
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(&body);
    write_all(channel, &frame, options, stop)
}
fn response(
    channel: &mut dyn Channel,
    id: u64,
    options: &JobOptions,
    stop: &AtomicBool,
) -> Result<Vec<u8>, StoreError> {
    let mut header = [0; 20];
    read_exact(channel, &mut header, options, stop)?;
    if &header[..4] != b"CSR4"
        || u64::from_le_bytes(header[4..12].try_into().unwrap()) != id
        || header[13..16] != [0; 3]
    {
        return Err(invalid("invalid store response"));
    }
    let len = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;
    if len > MAX_FRAME || header[12] > 8 {
        return Err(invalid("invalid store response bounds"));
    }
    let mut body = vec![0; len];
    read_exact(channel, &mut body, options, stop)?;
    if header[12] == 0 {
        Ok(body)
    } else {
        Err(code_error(header[12]))
    }
}

struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
fn admit(counter: &AtomicUsize) -> Result<Active<'_>, StoreError> {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < POOL_SIZE).then_some(n + 1)
        })
        .map_err(|_| StoreError::Busy)?;
    Ok(Active(counter))
}
/// One bounded client OS thread, also for async callers. Never calls block_on.
/// Cancellation closes the connection; a timed-out write has unknown outcome
/// until the caller re-reads its durable operation ID, never a blind retry.
pub fn request(
    endpoint: StoreEndpoint,
    request: Request,
    options: JobOptions,
) -> Result<JobHandle<Vec<u8>>, StoreError> {
    request.validate()?;
    let options = options_with_limit(options);
    options.check()?;
    let active = admit(&CLIENTS)?;
    let (send, receive) = oneshot::channel();
    // Active borrows a static counter and is safely transferred to the worker.
    thread::Builder::new()
        .name("ctox-store-client".into())
        .stack_size(256 * 1024)
        .spawn(move || {
            let _active = active;
            let stop = AtomicBool::new(false);
            let result = (|| {
                let mut channel = connect(&endpoint, &options, &stop)?;
                channel.authenticate(false)?;
                let frame = request_bytes(
                    &request,
                    options
                        .deadline
                        .unwrap()
                        .saturating_duration_since(Instant::now()),
                );
                write_all(&mut *channel, &frame, &options, &stop)?;
                response(&mut *channel, request.request_id, &options, &stop)
            })();
            let _ = send.send(result);
        })?;
    Ok(JobHandle(receive))
}

struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
fn serve_connection(
    mut channel: Box<dyn Channel>,
    accepted: Instant,
    dispatcher: &dyn Dispatcher,
    stop: &AtomicBool,
) {
    let mut options = JobOptions {
        deadline: Some(accepted + MAX_LIFETIME),
        ..Default::default()
    };
    let _cancel = CancelOnDrop(options.cancellation.clone());
    let request = match read_request(&mut *channel, &mut options, stop) {
        Ok(request) => request,
        Err(_) => return,
    };
    let id = request.request_id;
    let result = (|| {
        let mut job = dispatcher.dispatch(request, options.clone())?;
        loop {
            check(&options, stop)?;
            match job.0.try_recv() {
                Ok(result) => return result,
                Err(oneshot::error::TryRecvError::Closed) => return Err(StoreError::Closed),
                Err(oneshot::error::TryRecvError::Empty) => {}
            }
            let mut cancel = [0];
            match channel.read(&mut cancel) {
                Ok(0) => return Err(StoreError::Cancelled),
                Ok(_) => return Err(invalid("unexpected bytes after store request")),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
            pause(&options, stop)?;
        }
    })();
    // Cancel immediately on expiry/disconnect, not after a response drain.
    if result.is_err() {
        options.cancellation.cancel();
    }
    // Job cancellation must not suppress its own error response. Response I/O
    // has an independent token but retains the original absolute deadline.
    let response_options = JobOptions {
        deadline: options.deadline,
        ..Default::default()
    };
    let _ = respond(&mut *channel, id, result, &response_options, stop);
}

pub struct DaemonIpc {
    stop: Arc<AtomicBool>,
    acceptor: Option<thread::JoinHandle<Result<(), StoreError>>>,
}
impl DaemonIpc {
    /// Start before opening daemon databases, and keep through their shutdown.
    pub fn start(
        endpoint: StoreEndpoint,
        dispatcher: Arc<dyn Dispatcher>,
    ) -> Result<Self, StoreError> {
        let bound = Bound::bind(endpoint)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (send, receive) = mpsc::sync_channel::<(Box<dyn Channel>, Instant)>(POOL_SIZE);
        let receive = Arc::new(Mutex::new(receive));
        let mut workers = Vec::with_capacity(POOL_SIZE);
        for index in 0..POOL_SIZE {
            let receive = receive.clone();
            let dispatcher = dispatcher.clone();
            let stop = stop.clone();
            workers.push(
                thread::Builder::new()
                    .name(format!("ctox-store-ipc-{index}"))
                    .stack_size(256 * 1024)
                    .spawn(move || loop {
                        let next = receive.lock().unwrap_or_else(|e| e.into_inner()).recv();
                        let Ok((channel, accepted)) = next else { break };
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            serve_connection(channel, accepted, &*dispatcher, &stop)
                        }));
                    })?,
            );
        }
        let acceptor = thread::Builder::new()
            .name("ctox-store-ipc-accept".into())
            .spawn(move || {
                let mut bound = bound;
                let active = Arc::new(AtomicUsize::new(0));
                let mut result = Ok(());
                while !stopping.load(Ordering::Acquire) {
                    match bound.accept() {
                        Ok(channel) => {
                            if channel.authenticate(true).is_err() {
                                continue;
                            }
                            // Count all accepted/queued/active streams, not just busy workers.
                            if active
                                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                                    (n < POOL_SIZE).then_some(n + 1)
                                })
                                .is_err()
                            {
                                // No request is read or dispatched when saturated.
                                drop(channel);
                                continue;
                            }
                            let channel: Box<dyn Channel> = Box::new(CountedChannel {
                                channel,
                                active: active.clone(),
                            });
                            if send.try_send((channel, Instant::now())).is_err() {
                                continue;
                            }
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            thread::park_timeout(TICK)
                        }
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                        Err(error) => {
                            result = Err(error.into());
                            break;
                        }
                    }
                }
                stopping.store(true, Ordering::Release);
                drop(send);
                for worker in workers {
                    let _ = worker.join();
                }
                drop(bound);
                result
            })?;
        Ok(Self {
            stop,
            acceptor: Some(acceptor),
        })
    }
    /// The host supervises this flag as well as shutdown's failure result.
    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }
    pub fn shutdown(mut self) -> Result<JobHandle<()>, StoreError> {
        self.stop.store(true, Ordering::Release);
        let task = self.acceptor.take();
        let (send, receive) = oneshot::channel();
        thread::Builder::new()
            .name("ctox-store-ipc-close".into())
            .spawn(move || {
                let result = task.map_or(Ok(()), |task| {
                    task.join()
                        .map_err(|_| StoreError::Panicked)
                        .and_then(|result| result)
                });
                let _ = send.send(result);
            })?;
        Ok(JobHandle(receive))
    }
}
impl Drop for DaemonIpc {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
struct CountedChannel {
    channel: Box<dyn Channel>,
    active: Arc<AtomicUsize>,
}
impl Read for CountedChannel {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.channel.read(b)
    }
}
impl Write for CountedChannel {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.channel.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.channel.flush()
    }
}
impl Channel for CountedChannel {
    fn authenticate(&self, s: bool) -> io::Result<()> {
        self.channel.authenticate(s)
    }
}
impl Drop for CountedChannel {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

struct OwnerLock(File);
impl OwnerLock {
    fn acquire(path: &Path) -> Result<Self, StoreError> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.try_lock().map_err(|e| match e {
            fs::TryLockError::WouldBlock => StoreError::AlreadyOwned,
            fs::TryLockError::Error(e) => e.into(),
        })?;
        Ok(Self(file))
    }
}
impl Drop for OwnerLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// Explicit offline read-only exception. Acquires daemon-lifecycle ownership
/// first, then the *same* file lock as the writer. Wrong endpoint config must
/// never be accepted from an untrusted request; the native CLI resolves it.
pub struct MaintenanceOwner {
    _daemon: OwnerLock,
    _database: OwnerLock,
    id: DbId,
}
impl MaintenanceOwner {
    pub fn acquire(endpoint: &StoreEndpoint, id: DbId) -> Result<Self, StoreError> {
        private_directory(endpoint.directory()?)?;
        let daemon = OwnerLock::acquire(&endpoint.lock_path()?)?;
        let mut name = id.path().file_name().unwrap().to_os_string();
        name.push(".ctox-store.lock");
        let database = OwnerLock::acquire(&id.path().with_file_name(name))?;
        Ok(Self {
            _daemon: daemon,
            _database: database,
            id,
        })
    }
    /// Synchronous offline maintenance only, outside an async runtime.
    pub fn open_read_only(&self) -> Result<OfflineReader<'_>, StoreError> {
        let connection = rusqlite::Connection::open_with_flags(
            self.id.path(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        connection
            .execute_batch("PRAGMA query_only=ON; PRAGMA busy_timeout=0; PRAGMA foreign_keys=ON")?;
        Ok(OfflineReader {
            connection,
            _owner: self,
        })
    }
}
pub struct OfflineReader<'a> {
    connection: rusqlite::Connection,
    _owner: &'a MaintenanceOwner,
}
impl OfflineReader<'_> {
    pub fn read<T>(
        &self,
        operation: impl FnOnce(&rusqlite::Connection) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        operation(&self.connection)
    }
}

fn private_directory(path: &Path) -> Result<(), StoreError> {
    if !path.is_absolute() {
        return Err(invalid("IPC needs an absolute private directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_dir() || meta.uid() != unix::uid() || meta.mode() & 0o077 != 0 {
            return Err(invalid("IPC directory must be private and owned"));
        }
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

struct Bound {
    #[cfg(unix)]
    endpoint: StoreEndpoint,
    listener: Listener,
    _owner: OwnerLock,
    #[cfg(unix)]
    inode: Option<(u64, u64)>,
}
enum Listener {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixListener),
    #[cfg(windows)]
    Pipe(windows::PipeListener),
}
impl Bound {
    fn bind(endpoint: StoreEndpoint) -> Result<Self, StoreError> {
        private_directory(endpoint.directory()?)?;
        let owner = OwnerLock::acquire(&endpoint.lock_path()?)?;
        match &endpoint {
            #[cfg(unix)]
            StoreEndpoint::UnixSocket { path } => {
                use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
                match fs::symlink_metadata(path) {
                    Ok(meta) if meta.file_type().is_socket() && meta.uid() == unix::uid() => {
                        match unix::connect_path(
                            path,
                            Instant::now() + Duration::from_millis(100),
                            &AtomicBool::new(false),
                            &Cancellation::default(),
                        ) {
                            Ok(_) => return Err(StoreError::AlreadyOwned),
                            Err(e)
                                if matches!(
                                    e.kind(),
                                    io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                                ) =>
                            {
                                fs::remove_file(path)?
                            }
                            Err(e) => return Err(e.into()),
                        }
                    }
                    Ok(_) => return Err(invalid("refusing to replace foreign IPC path")),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
                let listener = std::os::unix::net::UnixListener::bind(path)?;
                fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
                listener.set_nonblocking(true)?;
                let meta = fs::symlink_metadata(path)?;
                let inode = Some((meta.dev(), meta.ino()));
                Ok(Self {
                    endpoint,
                    listener: Listener::Unix(listener),
                    _owner: owner,
                    inode,
                })
            }
            #[cfg(windows)]
            StoreEndpoint::NamedPipe { name, .. } => {
                let listener = windows::PipeListener::bind(name)?;
                Ok(Self {
                    listener: Listener::Pipe(listener),
                    _owner: owner,
                })
            }
            #[allow(unreachable_patterns)]
            _ => Err(invalid("IPC transport is unsupported on this platform")),
        }
    }
    fn accept(&mut self) -> io::Result<Box<dyn Channel>> {
        match &mut self.listener {
            #[cfg(unix)]
            Listener::Unix(listener) => {
                let (stream, _) = listener.accept()?;
                stream.set_nonblocking(true)?;
                Ok(Box::new(unix::Stream(stream)))
            }
            #[cfg(windows)]
            Listener::Pipe(listener) => listener.accept(),
        }
    }
}
impl Drop for Bound {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let StoreEndpoint::UnixSocket { path } = &self.endpoint {
            use std::os::unix::fs::MetadataExt;
            if fs::symlink_metadata(path)
                .ok()
                .is_some_and(|meta| Some((meta.dev(), meta.ino())) == self.inode)
            {
                let _ = fs::remove_file(path);
            }
        }
    }
}
fn connect(
    endpoint: &StoreEndpoint,
    options: &JobOptions,
    stop: &AtomicBool,
) -> Result<Box<dyn Channel>, StoreError> {
    check(options, stop)?;
    match endpoint {
        #[cfg(unix)]
        StoreEndpoint::UnixSocket { path } => {
            private_directory(endpoint.directory()?)?;
            let stream =
                unix::connect_path(path, options.deadline.unwrap(), stop, &options.cancellation)
                    .map_err(|error| options.check().err().unwrap_or_else(|| error.into()))?;
            Ok(Box::new(unix::Stream(stream)))
        }
        #[cfg(windows)]
        StoreEndpoint::NamedPipe { name, .. } => windows::connect(name, options, stop),
        #[allow(unreachable_patterns)]
        _ => Err(invalid("IPC transport is unsupported on this platform")),
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn geteuid() -> u32;
    }
    pub(super) fn uid() -> u32 {
        unsafe { geteuid() }
    }
    /// Non-blocking connect, including a full listen backlog. A blocking Unix
    /// connect would otherwise outlive the request deadline before we got an FD.
    pub(super) fn connect_path(
        path: &Path,
        deadline: Instant,
        stop: &AtomicBool,
        cancellation: &Cancellation,
    ) -> io::Result<std::os::unix::net::UnixStream> {
        use std::os::{fd::FromRawFd, unix::ffi::OsStrExt};
        unsafe extern "C" {
            fn socket(domain: i32, kind: i32, protocol: i32) -> i32;
            #[link_name = "connect"]
            fn os_connect(fd: i32, address: *const std::ffi::c_void, len: u32) -> i32;
            fn fcntl(fd: i32, command: i32, value: i32) -> i32;
            fn poll(fds: *mut PollFd, count: usize, timeout: i32) -> i32;
        }
        #[repr(C)]
        struct PollFd {
            fd: i32,
            events: i16,
            revents: i16,
        }
        #[cfg(target_os = "macos")]
        #[repr(C)]
        struct Address {
            len: u8,
            family: u8,
            path: [u8; 104],
        }
        #[cfg(not(target_os = "macos"))]
        #[repr(C)]
        struct Address {
            family: u16,
            path: [u8; 108],
        }
        let bytes = path.as_os_str().as_bytes();
        #[cfg(target_os = "macos")]
        let mut address = Address {
            len: 0,
            family: 1,
            path: [0; 104],
        };
        #[cfg(not(target_os = "macos"))]
        let mut address = Address {
            family: 1,
            path: [0; 108],
        };
        if bytes.len() >= address.path.len() || bytes.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid Unix socket path",
            ));
        }
        address.path[..bytes.len()].copy_from_slice(bytes);
        let len = (2 + bytes.len() + 1) as u32;
        #[cfg(target_os = "macos")]
        {
            address.len = len as u8;
        }
        let check = || {
            if stop.load(Ordering::Acquire) || cancellation.is_cancelled() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "IPC cancelled"));
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "IPC connect deadline",
                ));
            }
            Ok(())
        };
        loop {
            check()?;
            #[cfg(target_os = "linux")]
            let flags = 1 | 0x80000;
            #[cfg(not(target_os = "linux"))]
            let flags = 1;
            let fd = unsafe { socket(1, flags, 0) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // From this point the owned stream closes every failure path.
            let stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) };
            if unsafe { fcntl(fd, 2, 1) } < 0 {
                return Err(io::Error::last_os_error());
            }
            stream.set_nonblocking(true)?;
            if unsafe { os_connect(fd, (&address as *const Address).cast(), len) } == 0 {
                return Ok(stream);
            }
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::WouldBlock {
                drop(stream);
                thread::park_timeout(TICK.min(deadline.saturating_duration_since(Instant::now())));
                continue;
            }
            #[cfg(target_os = "macos")]
            let pending = error.raw_os_error() == Some(36);
            #[cfg(not(target_os = "macos"))]
            let pending = error.raw_os_error() == Some(115);
            if !pending {
                return Err(error);
            }
            let mut ready = PollFd {
                fd,
                events: 4,
                revents: 0,
            };
            loop {
                check()?;
                let wait = deadline
                    .saturating_duration_since(Instant::now())
                    .min(TICK)
                    .as_millis()
                    .max(1) as i32;
                let result = unsafe { poll(&mut ready, 1, wait) };
                if result < 0 {
                    let error = io::Error::last_os_error();
                    if error.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(error);
                }
                if result == 0 {
                    continue;
                }
                if let Some(error) = stream.take_error()? {
                    return Err(error);
                }
                stream.peer_addr()?;
                return Ok(stream);
            }
        }
    }
    pub(super) struct Stream(pub std::os::unix::net::UnixStream);
    impl Read for Stream {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            self.0.read(b)
        }
    }
    impl Write for Stream {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.write(b)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Channel for Stream {
        fn authenticate(&self, _: bool) -> io::Result<()> {
            self.authenticate_uid(uid())
        }
    }
    impl Stream {
        pub(super) fn authenticate_uid(&self, expected: u32) -> io::Result<()> {
            #[cfg(target_os = "linux")]
            let peer = {
                #[repr(C)]
                struct Credentials {
                    pid: i32,
                    uid: u32,
                    gid: u32,
                }
                unsafe extern "C" {
                    fn getsockopt(
                        fd: i32,
                        level: i32,
                        name: i32,
                        value: *mut std::ffi::c_void,
                        len: *mut u32,
                    ) -> i32;
                }
                let mut credentials = Credentials {
                    pid: 0,
                    uid: 0,
                    gid: 0,
                };
                let mut len = std::mem::size_of::<Credentials>() as u32;
                if unsafe {
                    getsockopt(
                        self.0.as_raw_fd(),
                        1,
                        17,
                        (&mut credentials as *mut Credentials).cast(),
                        &mut len,
                    )
                } != 0
                    || len as usize != std::mem::size_of::<Credentials>()
                {
                    return Err(io::Error::last_os_error());
                }
                credentials.uid
            };
            #[cfg(any(target_os = "macos", target_os = "freebsd"))]
            let peer = {
                unsafe extern "C" {
                    fn getpeereid(fd: i32, uid: *mut u32, gid: *mut u32) -> i32;
                }
                let mut peer = 0;
                let mut group = 0;
                if unsafe { getpeereid(self.0.as_raw_fd(), &mut peer, &mut group) } != 0 {
                    return Err(io::Error::last_os_error());
                }
                peer
            };
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "freebsd")))]
            {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "peer identity unavailable",
                ));
            }
            #[cfg(any(target_os = "linux", target_os = "macos", target_os = "freebsd"))]
            if peer != expected {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "foreign local IPC user",
                ));
            }
            Ok(())
        }
    }
}

#[cfg(windows)]
mod windows {
    // Win32 ABI references: CreateNamedPipeW, SetNamedPipeHandleState,
    // GetNamedPipe{Client,Server}ProcessId, OpenProcessToken, GetTokenInformation.
    // https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew
    // These are synchronous bounded pool workers; PIPE_NOWAIT is used only to
    // keep synchronous reads/writes under the common lifetime/cancel contract.
    use super::*;
    use std::{
        ffi::c_void,
        os::windows::io::{AsRawHandle, FromRawHandle},
    };
    type Handle = *mut c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateNamedPipeW(
            name: *const u16,
            open: u32,
            mode: u32,
            instances: u32,
            out: u32,
            input: u32,
            timeout: u32,
            security: *const c_void,
        ) -> Handle;
        fn ConnectNamedPipe(pipe: Handle, overlapped: *mut c_void) -> i32;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *const c_void,
            creation: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
        fn SetNamedPipeHandleState(
            pipe: Handle,
            mode: *const u32,
            count: *const u32,
            timeout: *const u32,
        ) -> i32;
        fn GetNamedPipeClientProcessId(pipe: Handle, pid: *mut u32) -> i32;
        fn GetNamedPipeServerProcessId(pipe: Handle, pid: *mut u32) -> i32;
        fn GetCurrentProcess() -> Handle;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        fn GetTokenInformation(
            token: Handle,
            class: u32,
            buffer: *mut c_void,
            len: u32,
            needed: *mut u32,
        ) -> i32;
        fn EqualSid(first: *const c_void, second: *const c_void) -> i32;
    }
    struct Owned(Handle);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    fn token_user(process: Handle) -> io::Result<Vec<u8>> {
        let mut token = std::ptr::null_mut();
        if unsafe { OpenProcessToken(process, 8, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = Owned(token);
        let mut needed = 0;
        unsafe {
            GetTokenInformation(token.0, 1, std::ptr::null_mut(), 0, &mut needed);
        }
        if needed < std::mem::size_of::<Handle>() as u32 || needed > 65536 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "invalid peer token",
            ));
        }
        let mut bytes = vec![0; needed as usize];
        if unsafe {
            GetTokenInformation(token.0, 1, bytes.as_mut_ptr().cast(), needed, &mut needed)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(bytes)
    }
    fn pipe_path(name: &str) -> io::Result<Vec<u16>> {
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid private pipe name",
            ));
        }
        Ok(format!(r"\\.\pipe\{name}")
            .encode_utf16()
            .chain(Some(0))
            .collect())
    }
    pub(super) struct Pipe(File);
    fn nonblocking(error: io::Error) -> io::Error {
        if matches!(error.raw_os_error(), Some(232 | 536)) {
            io::ErrorKind::WouldBlock.into()
        } else {
            error
        }
    }
    impl Read for Pipe {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            match self.0.read(b) {
                Err(e) if e.raw_os_error() == Some(109) => Ok(0),
                other => other.map_err(nonblocking),
            }
        }
    }
    impl Write for Pipe {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.write(b).map_err(nonblocking).and_then(|n| {
                if n == 0 && !b.is_empty() {
                    Err(io::ErrorKind::WouldBlock.into())
                } else {
                    Ok(n)
                }
            })
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Channel for Pipe {
        fn authenticate(&self, server: bool) -> io::Result<()> {
            let mut pid = 0;
            let handle = self.0.as_raw_handle();
            let ok = if server {
                unsafe { GetNamedPipeClientProcessId(handle, &mut pid) }
            } else {
                unsafe { GetNamedPipeServerProcessId(handle, &mut pid) }
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            let peer = unsafe { OpenProcess(0x1000, 0, pid) };
            if peer.is_null() {
                return Err(io::Error::last_os_error());
            }
            let peer = Owned(peer);
            let current = token_user(unsafe { GetCurrentProcess() })?;
            let remote = token_user(peer.0)?;
            // TOKEN_USER starts with SID_AND_ATTRIBUTES.Sid. Read the pointer
            // unaligned; both complete token buffers stay alive through EqualSid.
            let current_sid = unsafe { current.as_ptr().cast::<Handle>().read_unaligned() };
            let remote_sid = unsafe { remote.as_ptr().cast::<Handle>().read_unaligned() };
            if unsafe { EqualSid(current_sid, remote_sid) } == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "foreign local IPC user",
                ));
            }
            Ok(())
        }
    }
    pub(super) struct PipeListener {
        name: String,
        pending: File,
    }
    impl PipeListener {
        fn instance(name: &str, first: bool) -> io::Result<File> {
            let path = pipe_path(name)?;
            let handle = unsafe {
                CreateNamedPipeW(
                    path.as_ptr(),
                    3 | if first { 0x80000 } else { 0 },
                    1 | 8,
                    255,
                    65536,
                    65536,
                    0,
                    std::ptr::null(),
                )
            };
            if handle as isize == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(unsafe { File::from_raw_handle(handle) })
        }
        pub(super) fn bind(name: &str) -> io::Result<Self> {
            Ok(Self {
                name: name.into(),
                pending: Self::instance(name, true)?,
            })
        }
        pub(super) fn accept(&mut self) -> io::Result<Box<dyn Channel>> {
            let ok =
                unsafe { ConnectNamedPipe(self.pending.as_raw_handle(), std::ptr::null_mut()) };
            if ok == 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(535) {
                    return Err(nonblocking(error));
                }
            }
            let next = Self::instance(&self.name, false)?;
            let current = std::mem::replace(&mut self.pending, next);
            Ok(Box::new(Pipe(current)))
        }
    }
    pub(super) fn connect(
        name: &str,
        options: &JobOptions,
        stop: &AtomicBool,
    ) -> Result<Box<dyn Channel>, StoreError> {
        let path = pipe_path(name)?;
        loop {
            check(options, stop)?;
            let handle = unsafe {
                CreateFileW(
                    path.as_ptr(),
                    0xc0000000,
                    0,
                    std::ptr::null(),
                    3,
                    0,
                    std::ptr::null_mut(),
                )
            };
            if handle as isize != -1 {
                let file = unsafe { File::from_raw_handle(handle) };
                let mode = 1u32;
                if unsafe {
                    SetNamedPipeHandleState(handle, &mode, std::ptr::null(), std::ptr::null())
                } == 0
                {
                    return Err(io::Error::last_os_error().into());
                }
                return Ok(Box::new(Pipe(file)));
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(231) {
                return Err(error.into());
            }
            pause(options, stop)?;
        }
    }
}

#[cfg(test)]
#[path = "ipc_tests.rs"]
mod tests;
