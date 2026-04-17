// Origin: CTOX
// License: Apache-2.0

//! Portable transport abstraction for local inference backends.
//!
//! CTOX talks to locally-spawned inference processes over IPC. Historically
//! this code used `std::os::unix::net::UnixStream` directly, which pins the
//! product to macOS and Linux. This module introduces a `LocalTransport` enum
//! plus a `LocalStream` handle so the same client logic runs on Windows as
//! well.
//!
//! Variants:
//! - [`LocalTransport::UnixSocket`] — Unix domain socket (macOS, Linux).
//! - [`LocalTransport::NamedPipe`] — Windows named pipe (real impl on Windows
//!   via the `windows-sys` crate; returns `Unsupported` elsewhere).
//! - [`LocalTransport::TcpLoopback`] — TCP on loopback; universal fallback.
//!
//! # Migration notes for callers
//!
//! Replace a code block like:
//! ```ignore
//! let stream = UnixStream::connect(&socket_path)?;
//! stream.set_read_timeout(Some(timeout))?;
//! stream.set_write_timeout(Some(timeout))?;
//! ```
//!
//! with:
//! ```ignore
//! let stream = transport.connect_blocking(timeout)?;
//! ```
//!
//! The returned [`LocalStream`] implements `Read + Write`, so `BufReader`,
//! `write_all`, and other `std::io` consumers work unchanged.

#[cfg(unix)]
use std::fs;
use std::io;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::net::UnixListener;
#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[cfg(windows)]
mod named_pipe {
    //! Blocking named-pipe client + single-instance server for Windows.
    //!
    //! Timeouts on read/write are best-effort no-ops: stdlib's `File` on a
    //! named-pipe HANDLE does not expose `SetCommTimeouts`, so the
    //! `set_read_timeout`/`set_write_timeout` calls in [`LocalStream`] are
    //! silently ignored for this transport. Callers that need hard timeouts
    //! on Windows should prefer `TcpLoopback`.
    use std::ffi::c_void;
    use std::fs::File;
    use std::io;
    use std::os::windows::io::FromRawHandle;
    use std::os::windows::io::OwnedHandle;
    use std::time::Instant;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, GENERIC_READ, GENERIC_WRITE,
        INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    const BUFFER_SIZE: u32 = 64 * 1024;

    fn pipe_path(name: &str) -> Vec<u16> {
        let full = format!(r"\\.\pipe\{name}");
        full.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn last_os_error() -> io::Error {
        io::Error::last_os_error()
    }

    /// Open a connection to an existing named pipe. Retries briefly if all
    /// pipe instances are busy — this matches the `WaitNamedPipe` retry loop
    /// callers would otherwise have to write.
    pub fn connect(name: &str, timeout: Duration) -> io::Result<File> {
        let path = pipe_path(name);
        let deadline = Instant::now() + timeout;
        loop {
            let handle = unsafe {
                CreateFileW(
                    path.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut::<c_void>() as _,
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                // SAFETY: handle came from a successful CreateFileW and is
                // not used elsewhere. OwnedHandle owns + closes it on drop.
                let owned = unsafe { OwnedHandle::from_raw_handle(handle as _) };
                return Ok(File::from(owned));
            }
            let err = last_os_error();
            let raw = err.raw_os_error().unwrap_or(0) as u32;
            if raw == ERROR_FILE_NOT_FOUND || Instant::now() >= deadline {
                return Err(err);
            }
            if raw == ERROR_PIPE_BUSY {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
            return Err(err);
        }
    }

    /// A single named-pipe server instance. `accept` blocks on
    /// `ConnectNamedPipe`; on success the current instance is handed to the
    /// caller and a fresh instance is pre-created for the next accept.
    pub struct Server {
        name: String,
        pending_handle: isize,
    }

    impl Server {
        pub fn bind(name: &str) -> io::Result<Self> {
            let pending_handle = create_instance(name)?;
            Ok(Self {
                name: name.to_string(),
                pending_handle,
            })
        }

        pub fn accept(&mut self) -> io::Result<File> {
            let current = self.pending_handle;
            let ok = unsafe { ConnectNamedPipe(current as _, std::ptr::null_mut()) };
            // ConnectNamedPipe returns 0 on failure. ERROR_PIPE_CONNECTED
            // (535) means a client raced us and is already connected; both
            // outcomes yield a usable handle.
            if ok == 0 {
                let err = last_os_error();
                let raw = err.raw_os_error().unwrap_or(0);
                if raw != 535 {
                    // Close the failed handle; replace pending so the next
                    // accept can retry without leaking.
                    unsafe { CloseHandle(current as _) };
                    self.pending_handle = create_instance(&self.name)?;
                    return Err(err);
                }
            }
            // Pre-create the next instance before handing this one off.
            self.pending_handle = create_instance(&self.name)?;
            let owned = unsafe { OwnedHandle::from_raw_handle(current as _) };
            Ok(File::from(owned))
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            if self.pending_handle != 0 && self.pending_handle != INVALID_HANDLE_VALUE as isize {
                unsafe { CloseHandle(self.pending_handle as _) };
            }
        }
    }

    fn create_instance(name: &str) -> io::Result<isize> {
        let path = pipe_path(name);
        let handle = unsafe {
            CreateNamedPipeW(
                path.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER_SIZE,
                BUFFER_SIZE,
                0,
                std::ptr::null_mut::<c_void>() as _,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(last_os_error());
        }
        Ok(handle as isize)
    }

    /// Lightweight probe: open + close. Cheaper than a real I/O exchange.
    pub fn probe(name: &str) -> bool {
        connect(name, Duration::from_millis(100)).is_ok()
    }
}

const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// Endpoint that a local inference backend listens on.
///
/// This value is cheap to clone and can be embedded inside runtime-binding
/// structs. The variants are `cfg`-independent on purpose so the enum stays
/// exhaustive across platforms; connects on an unsupported platform return
/// `io::ErrorKind::Unsupported` instead of being removed from the enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalTransport {
    /// Unix domain socket at `path`.
    UnixSocket { path: PathBuf },
    /// Windows named pipe, e.g. `ctox-primary-generation`. The full pipe path
    /// (`\\.\pipe\<name>`) is derived inside the connector.
    NamedPipe { name: String },
    /// TCP on loopback. Works on every platform and is the Windows default
    /// until named-pipe support lands.
    TcpLoopback { host: String, port: u16 },
}

impl LocalTransport {
    /// Choose a sensible default transport for the current platform.
    ///
    /// `unix_socket_path` is used on Unix if provided; `tcp_port` seeds the
    /// loopback fallback everywhere else. The intent is that callers can keep
    /// passing the same inputs they compute today and get the right transport
    /// for the host without branching themselves.
    pub fn default_for_host(
        unix_socket_path: Option<PathBuf>,
        tcp_host: &str,
        tcp_port: u16,
    ) -> Self {
        #[cfg(unix)]
        {
            if let Some(path) = unix_socket_path {
                return Self::UnixSocket { path };
            }
        }
        #[cfg(not(unix))]
        {
            let _ = unix_socket_path;
        }
        Self::TcpLoopback {
            host: tcp_host.to_string(),
            port: tcp_port,
        }
    }

    /// Short human-readable label for logs and UI, e.g.
    /// `unix:/tmp/ctox/primary.sock` or `tcp:127.0.0.1:2234`.
    pub fn display_label(&self) -> String {
        match self {
            Self::UnixSocket { path } => format!("unix:{}", path.display()),
            Self::NamedPipe { name } => format!("pipe:\\\\.\\pipe\\{name}"),
            Self::TcpLoopback { host, port } => format!("tcp:{host}:{port}"),
        }
    }

    /// Returns the HTTP base URL for transports that speak TCP.
    /// IPC transports (Unix socket, named pipe) return `None` — callers that
    /// need HTTP must connect via [`connect_blocking`](Self::connect_blocking)
    /// and speak the framed protocol instead.
    pub fn http_base_url(&self) -> Option<String> {
        match self {
            Self::TcpLoopback { host, port } => Some(format!("http://{host}:{port}")),
            Self::UnixSocket { .. } | Self::NamedPipe { .. } => None,
        }
    }

    /// Filesystem path for Unix-socket transports. Helper for legacy code
    /// paths still keyed off `Option<String>` socket paths — prefer
    /// [`connect_blocking`](Self::connect_blocking) in new code.
    pub fn unix_socket_path(&self) -> Option<&std::path::Path> {
        match self {
            Self::UnixSocket { path } => Some(path.as_path()),
            _ => None,
        }
    }

    /// Open a synchronous, blocking stream with the given read/write timeout.
    pub fn connect_blocking(&self, timeout: Duration) -> io::Result<LocalStream> {
        match self {
            Self::UnixSocket { path } => {
                #[cfg(unix)]
                {
                    let stream = UnixStream::connect(path)?;
                    stream.set_read_timeout(Some(timeout))?;
                    stream.set_write_timeout(Some(timeout))?;
                    Ok(LocalStream {
                        inner: StreamInner::Unix(stream),
                    })
                }
                #[cfg(not(unix))]
                {
                    let _ = (path, timeout);
                    Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "UnixSocket transport requires a Unix platform",
                    ))
                }
            }
            Self::NamedPipe { name } => {
                #[cfg(windows)]
                {
                    let file = named_pipe::connect(name, timeout)?;
                    Ok(LocalStream {
                        inner: StreamInner::NamedPipe(file),
                    })
                }
                #[cfg(not(windows))]
                {
                    let _ = (name, timeout);
                    Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "NamedPipe transport is only available on Windows",
                    ))
                }
            }
            Self::TcpLoopback { host, port } => {
                let addr = (host.as_str(), *port)
                    .to_socket_addrs()?
                    .next()
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("no socket address resolved for {host}:{port}"),
                        )
                    })?;
                let stream = TcpStream::connect_timeout(&addr, timeout)?;
                stream.set_read_timeout(Some(timeout))?;
                stream.set_write_timeout(Some(timeout))?;
                Ok(LocalStream {
                    inner: StreamInner::Tcp(stream),
                })
            }
        }
    }

    /// Bind a listener on this transport. Creates parent directories and
    /// replaces stale Unix-socket inodes for the `UnixSocket` variant.
    pub fn bind(&self) -> io::Result<LocalListener> {
        match self {
            Self::UnixSocket { path } => {
                #[cfg(unix)]
                {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    // Best-effort cleanup of a stale socket inode from a
                    // previous run. Ignore NotFound; propagate anything else.
                    if let Err(err) = fs::remove_file(path) {
                        if err.kind() != io::ErrorKind::NotFound {
                            return Err(err);
                        }
                    }
                    let listener = UnixListener::bind(path)?;
                    Ok(LocalListener {
                        inner: ListenerInner::Unix(listener),
                        label: self.display_label(),
                    })
                }
                #[cfg(not(unix))]
                {
                    let _ = path;
                    Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "UnixSocket transport requires a Unix platform",
                    ))
                }
            }
            Self::NamedPipe { name } => {
                #[cfg(windows)]
                {
                    let server = named_pipe::Server::bind(name)?;
                    Ok(LocalListener {
                        inner: ListenerInner::NamedPipe(server),
                        label: self.display_label(),
                    })
                }
                #[cfg(not(windows))]
                {
                    let _ = name;
                    Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "NamedPipe transport is only available on Windows",
                    ))
                }
            }
            Self::TcpLoopback { host, port } => {
                let addr = (host.as_str(), *port)
                    .to_socket_addrs()?
                    .next()
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("no socket address resolved for {host}:{port}"),
                        )
                    })?;
                let listener = TcpListener::bind(addr)?;
                Ok(LocalListener {
                    inner: ListenerInner::Tcp(listener),
                    label: self.display_label(),
                })
            }
        }
    }

    /// Lightweight liveness probe. Returns `true` if a connection attempt
    /// succeeds within a bounded timeout.
    pub fn probe(&self) -> bool {
        match self {
            Self::UnixSocket { path } => {
                #[cfg(unix)]
                {
                    UnixStream::connect(path).is_ok()
                }
                #[cfg(not(unix))]
                {
                    let _ = path;
                    false
                }
            }
            Self::NamedPipe { name } => {
                #[cfg(windows)]
                {
                    named_pipe::probe(name)
                }
                #[cfg(not(windows))]
                {
                    let _ = name;
                    false
                }
            }
            Self::TcpLoopback { host, port } => {
                let Some(addr) = (host.as_str(), *port).to_socket_addrs().ok().and_then(|mut iter| iter.next())
                else {
                    return false;
                };
                TcpStream::connect_timeout(&addr, PROBE_TIMEOUT).is_ok()
            }
        }
    }
}

/// Opaque blocking stream handle returned by [`LocalTransport::connect_blocking`]
/// or [`LocalListener::accept`].
///
/// Implements `Read + Write`, so `BufReader`, `write_all`, `flush`, etc. work
/// transparently regardless of the underlying transport.
pub struct LocalStream {
    inner: StreamInner,
}

enum StreamInner {
    #[cfg(unix)]
    Unix(UnixStream),
    Tcp(TcpStream),
    #[cfg(windows)]
    NamedPipe(std::fs::File),
}

impl LocalStream {
    /// Duplicate the stream handle so reader and writer halves can be used
    /// independently.
    pub fn try_clone(&self) -> io::Result<LocalStream> {
        let inner = match &self.inner {
            #[cfg(unix)]
            StreamInner::Unix(stream) => StreamInner::Unix(stream.try_clone()?),
            StreamInner::Tcp(stream) => StreamInner::Tcp(stream.try_clone()?),
            #[cfg(windows)]
            StreamInner::NamedPipe(file) => StreamInner::NamedPipe(file.try_clone()?),
        };
        Ok(LocalStream { inner })
    }

    /// Sets the read timeout for the underlying transport.
    ///
    /// On Windows named-pipe streams this is a best-effort no-op because the
    /// stdlib `File` wrapper does not expose the underlying `SetCommTimeouts`.
    /// Callers that need hard timeouts on Windows should use `TcpLoopback`.
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match &self.inner {
            #[cfg(unix)]
            StreamInner::Unix(stream) => stream.set_read_timeout(timeout),
            StreamInner::Tcp(stream) => stream.set_read_timeout(timeout),
            #[cfg(windows)]
            StreamInner::NamedPipe(_) => {
                let _ = timeout;
                Ok(())
            }
        }
    }

    /// See [`set_read_timeout`](Self::set_read_timeout) for Windows caveat.
    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match &self.inner {
            #[cfg(unix)]
            StreamInner::Unix(stream) => stream.set_write_timeout(timeout),
            StreamInner::Tcp(stream) => stream.set_write_timeout(timeout),
            #[cfg(windows)]
            StreamInner::NamedPipe(_) => {
                let _ = timeout;
                Ok(())
            }
        }
    }
}

impl Read for LocalStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.inner {
            #[cfg(unix)]
            StreamInner::Unix(stream) => stream.read(buf),
            StreamInner::Tcp(stream) => stream.read(buf),
            #[cfg(windows)]
            StreamInner::NamedPipe(file) => file.read(buf),
        }
    }
}

impl Write for LocalStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.inner {
            #[cfg(unix)]
            StreamInner::Unix(stream) => stream.write(buf),
            StreamInner::Tcp(stream) => stream.write(buf),
            #[cfg(windows)]
            StreamInner::NamedPipe(file) => file.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.inner {
            #[cfg(unix)]
            StreamInner::Unix(stream) => stream.flush(),
            StreamInner::Tcp(stream) => stream.flush(),
            #[cfg(windows)]
            StreamInner::NamedPipe(file) => file.flush(),
        }
    }
}

/// Blocking listener returned by [`LocalTransport::bind`].
pub struct LocalListener {
    inner: ListenerInner,
    label: String,
}

enum ListenerInner {
    #[cfg(unix)]
    Unix(UnixListener),
    Tcp(TcpListener),
    #[cfg(windows)]
    NamedPipe(named_pipe::Server),
}

impl LocalListener {
    /// Block until the next connection arrives; returns a `LocalStream` for
    /// the accepted peer. Takes `&mut` because the Windows named-pipe backend
    /// rotates its pending pipe instance after every connect.
    pub fn accept(&mut self) -> io::Result<LocalStream> {
        match &mut self.inner {
            #[cfg(unix)]
            ListenerInner::Unix(listener) => {
                let (stream, _) = listener.accept()?;
                Ok(LocalStream {
                    inner: StreamInner::Unix(stream),
                })
            }
            ListenerInner::Tcp(listener) => {
                let (stream, _) = listener.accept()?;
                Ok(LocalStream {
                    inner: StreamInner::Tcp(stream),
                })
            }
            #[cfg(windows)]
            ListenerInner::NamedPipe(server) => {
                let file = server.accept()?;
                Ok(LocalStream {
                    inner: StreamInner::NamedPipe(file),
                })
            }
        }
    }

    /// Human-readable label of the bound endpoint (matches the originating
    /// `LocalTransport::display_label` at bind time).
    pub fn display_label(&self) -> &str {
        &self.label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_label_formats_each_variant() {
        let unix = LocalTransport::UnixSocket {
            path: PathBuf::from("/tmp/ctox.sock"),
        };
        assert_eq!(unix.display_label(), "unix:/tmp/ctox.sock");

        let pipe = LocalTransport::NamedPipe {
            name: "ctox-primary".to_string(),
        };
        assert_eq!(pipe.display_label(), "pipe:\\\\.\\pipe\\ctox-primary");

        let tcp = LocalTransport::TcpLoopback {
            host: "127.0.0.1".to_string(),
            port: 2234,
        };
        assert_eq!(tcp.display_label(), "tcp:127.0.0.1:2234");
    }

    #[test]
    fn http_base_url_only_tcp() {
        let tcp = LocalTransport::TcpLoopback {
            host: "127.0.0.1".to_string(),
            port: 2234,
        };
        assert_eq!(
            tcp.http_base_url().as_deref(),
            Some("http://127.0.0.1:2234")
        );

        assert!(
            LocalTransport::UnixSocket {
                path: PathBuf::from("/tmp/x.sock"),
            }
            .http_base_url()
            .is_none()
        );
        assert!(
            LocalTransport::NamedPipe {
                name: "x".to_string(),
            }
            .http_base_url()
            .is_none()
        );
    }

    #[test]
    fn default_for_host_prefers_unix_socket_when_available_on_unix() {
        let path = PathBuf::from("/tmp/ctox.sock");
        let transport = LocalTransport::default_for_host(Some(path.clone()), "127.0.0.1", 2234);
        #[cfg(unix)]
        assert_eq!(transport, LocalTransport::UnixSocket { path });
        #[cfg(not(unix))]
        assert_eq!(
            transport,
            LocalTransport::TcpLoopback {
                host: "127.0.0.1".to_string(),
                port: 2234,
            }
        );
    }

    #[test]
    fn default_for_host_falls_back_to_tcp_when_no_socket_path() {
        let transport = LocalTransport::default_for_host(None, "127.0.0.1", 2234);
        assert_eq!(
            transport,
            LocalTransport::TcpLoopback {
                host: "127.0.0.1".to_string(),
                port: 2234,
            }
        );
    }

    #[test]
    fn named_pipe_connect_returns_unsupported() {
        let transport = LocalTransport::NamedPipe {
            name: "ctox-x".to_string(),
        };
        let result = transport.connect_blocking(Duration::from_millis(10));
        let err = match result {
            Ok(_) => panic!("named-pipe connect should return Unsupported, not succeed"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    }

    #[cfg(not(unix))]
    #[test]
    fn unix_socket_connect_on_non_unix_returns_unsupported() {
        let transport = LocalTransport::UnixSocket {
            path: PathBuf::from(r"C:\\tmp\\fake.sock"),
        };
        let result = transport.connect_blocking(Duration::from_millis(10));
        let err = match result {
            Ok(_) => panic!("unix-socket connect on non-unix should return Unsupported"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    }

    #[test]
    fn probe_rejects_invalid_tcp_host() {
        let transport = LocalTransport::TcpLoopback {
            host: "definitely-not-a-valid-host.invalid".to_string(),
            port: 65000,
        };
        assert!(!transport.probe());
    }

    fn run_roundtrip(
        bind_transport: LocalTransport,
        client_factory: impl FnOnce(&LocalListener) -> LocalTransport + Send + 'static,
    ) {
        let mut listener = bind_transport.bind().expect("bind should succeed");
        let client_transport = client_factory(&listener);

        let server = std::thread::spawn(move || {
            let write_stream = listener.accept().expect("accept");
            let read_stream = write_stream.try_clone().expect("server clone");
            let mut reader = std::io::BufReader::new(read_stream);
            let mut writer = write_stream;
            let mut line = String::new();
            std::io::BufRead::read_line(&mut reader, &mut line).expect("server read");
            assert_eq!(line.trim(), r#"{"ping":1}"#);
            writer.write_all(b"{\"pong\":1}\n").expect("server write");
            writer.flush().expect("server flush");
        });

        let write_stream = client_transport
            .connect_blocking(Duration::from_secs(2))
            .expect("connect");
        let read_stream = write_stream.try_clone().expect("client clone");
        let mut writer = write_stream;
        writer.write_all(b"{\"ping\":1}\n").expect("client write");
        writer.flush().expect("client flush");
        let mut reader = std::io::BufReader::new(read_stream);
        let mut response = String::new();
        std::io::BufRead::read_line(&mut reader, &mut response).expect("client read");
        assert_eq!(response.trim(), r#"{"pong":1}"#);

        server.join().expect("server thread");
    }

    #[test]
    fn tcp_loopback_listener_accepts_and_streams_newline_json() {
        // End-to-end roundtrip over TcpLoopback — the transport Windows uses
        // when Unix sockets are unavailable.
        run_roundtrip(
            LocalTransport::TcpLoopback {
                host: "127.0.0.1".to_string(),
                port: 0, // OS-assigned
            },
            |listener| {
                let bound = match &listener.inner {
                    ListenerInner::Tcp(tcp) => tcp.local_addr().expect("local_addr"),
                    #[cfg(unix)]
                    _ => unreachable!("expected Tcp listener"),
                };
                LocalTransport::TcpLoopback {
                    host: bound.ip().to_string(),
                    port: bound.port(),
                }
            },
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_socket_listener_accepts_and_streams_newline_json() {
        let dir = std::env::temp_dir().join(format!(
            "ctox-lt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.sock");
        let path_for_client = path.clone();
        run_roundtrip(
            LocalTransport::UnixSocket { path: path.clone() },
            move |_| LocalTransport::UnixSocket {
                path: path_for_client,
            },
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
