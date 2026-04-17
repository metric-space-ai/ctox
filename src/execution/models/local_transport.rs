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
//! - [`LocalTransport::NamedPipe`] — Windows named pipe (placeholder; a full
//!   implementation arrives with the Windows port).
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

use std::io;
use std::io::Read;
use std::io::Write;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

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
                let _ = (name, timeout);
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "NamedPipe transport is not implemented yet; use TcpLoopback on Windows",
                ))
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
            Self::NamedPipe { .. } => false,
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

/// Opaque blocking stream handle returned by [`LocalTransport::connect_blocking`].
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
}

impl Read for LocalStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.inner {
            #[cfg(unix)]
            StreamInner::Unix(stream) => stream.read(buf),
            StreamInner::Tcp(stream) => stream.read(buf),
        }
    }
}

impl Write for LocalStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.inner {
            #[cfg(unix)]
            StreamInner::Unix(stream) => stream.write(buf),
            StreamInner::Tcp(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.inner {
            #[cfg(unix)]
            StreamInner::Unix(stream) => stream.flush(),
            StreamInner::Tcp(stream) => stream.flush(),
        }
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
        let err = transport.connect_blocking(Duration::from_millis(10)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    }

    #[cfg(not(unix))]
    #[test]
    fn unix_socket_connect_on_non_unix_returns_unsupported() {
        let transport = LocalTransport::UnixSocket {
            path: PathBuf::from(r"C:\\tmp\\fake.sock"),
        };
        let err = transport.connect_blocking(Duration::from_millis(10)).unwrap_err();
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
}
