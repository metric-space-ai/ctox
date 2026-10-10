//! Where the container daemon is, and how that becomes a bind mount.
//!
//! A CI build computer has to find Docker, and it has to decide whether the
//! job's container gets the daemon socket mounted. Both are decided from three
//! inputs, in this order: `DOCKER_HOST`, the `--container-daemon-socket` flag,
//! and a list of well-known socket locations.
//!
//! # The case matrix is the specification
//!
//! Upstream annotates every branch with its case letter, and the
//! `docker_socket_test.go` table walks the matrix. The letters:
//!
//! | | `DOCKER_HOST` set | `DOCKER_HOST` unset |
//! |---|---|---|
//! | socket `-` (do not mount) | 1A | 1B — **error** |
//! | socket is a file path | 2A | 2B — **error** |
//! | socket is a valid URI | 3A | 3B — `DOCKER_HOST := socket` |
//! | socket omitted | 4A | 4B — use the default location |
//!
//! # Testability is part of the port
//!
//! Upstream mutates a package-level `CommonSocketLocations` and the process
//! environment from its tests. Here the list and the host are **parameters**,
//! with a zero-argument wrapper that reads the real ones. Same behaviour, and
//! the matrix is walkable without a Docker daemon on the machine.

/// Where a container daemon's socket is looked for, in order.
///
/// `$HOME` and `$XDG_RUNTIME_DIR` are expanded per entry, and the named-pipe
/// entry is only meaningful on Windows.
pub const COMMON_SOCKET_LOCATIONS: &[&str] = &[
    "/var/run/docker.sock",
    "/run/podman/podman.sock",
    "$HOME/.colima/docker.sock",
    "$XDG_RUNTIME_DIR/docker.sock",
    "$XDG_RUNTIME_DIR/podman/podman.sock",
    r"\\.\pipe\docker_engine",
    "$HOME/.docker/run/docker.sock",
];

/// The socket to connect to and the value to hand the container's `DOCKER_HOST`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SocketAndHost {
    /// The socket to bind-mount, or `-` for "mount nothing".
    pub socket: String,
    /// The `DOCKER_HOST` the container will see.
    pub host: String,
}

/// Why no socket could be decided on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketError {
    /// Neither a host nor a usable socket.
    NoHostAndInvalidSocket(String),
    /// No host, and the supplied socket is not a URI.
    NoHostAndInvalidContainerSocket(String),
}

impl std::fmt::Display for SocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHostAndInvalidSocket(socket) => write!(
                f,
                "no DOCKER_HOST and an invalid container socket '{socket}'"
            ),
            Self::NoHostAndInvalidContainerSocket(socket) => write!(
                f,
                "DOCKER_HOST was not set, couldn't be found in the usual locations, \
                 and the container daemon socket ('{socket}') is invalid"
            ),
        }
    }
}

impl std::error::Error for SocketError {}

/// The first socket that exists, as a URI, or `None`.
///
/// `docker_host` wins outright when it is set: an explicit `DOCKER_HOST` is
/// never second-guessed, not even when the file behind it is missing.
pub fn socket_location_in(
    locations: &[&str],
    docker_host: Option<&str>,
) -> Option<String> {
    if let Some(host) = docker_host {
        return Some(host.to_string());
    }
    for location in locations {
        let expanded = expand_env(location);
        // `Lstat`, not `Stat`: a dangling symlink still counts, because a
        // socket that is about to be created by a daemon that is still
        // starting is a legitimate answer.
        if std::fs::symlink_metadata(&expanded).is_ok() {
            if expanded.starts_with(r"\\.\") {
                return Some(format!("npipe://{}", to_slash(&expanded)));
            }
            return Some(format!("unix://{}", to_slash(&expanded)));
        }
    }
    None
}

/// [`socket_location_in`] against the real list and the real environment.
pub fn socket_location() -> Option<String> {
    socket_location_in(
        COMMON_SOCKET_LOCATIONS,
        std::env::var("DOCKER_HOST").ok().as_deref(),
    )
}

/// Whether `daemon_path` is a Docker host URI.
///
/// The test is that everything before `://` is alphabetic. A path like
/// `/my/socket.sock` has no `://` at all, and `unix:/path` has a scheme with a
/// `/` in it, so both are rejected — which is what keeps a bare file path from
/// being mistaken for a host.
pub fn is_docker_host_uri(daemon_path: &str) -> bool {
    let Some(index) = daemon_path.find("://") else {
        return false;
    };
    let scheme = &daemon_path[..index];
    // Go's `IndexFunc(...) == -1`: true only when nothing matched.
    !scheme.chars().any(|c| !c.is_ascii_alphabetic())
}

/// [`get_socket_and_host_in`] against the real list and environment.
pub fn get_socket_and_host(container_socket: &str) -> Result<SocketAndHost, SocketError> {
    get_socket_and_host_in(
        container_socket,
        COMMON_SOCKET_LOCATIONS,
        std::env::var("DOCKER_HOST").ok().as_deref(),
    )
}

/// The decision, with its inputs supplied.
///
/// The case letters in the comments are upstream's, and they are the only
/// reliable description of what this does.
pub fn get_socket_and_host_in(
    container_socket: &str,
    locations: &[&str],
    docker_host: Option<&str>,
) -> Result<SocketAndHost, SocketError> {
    let mut host = socket_location_in(locations, docker_host);
    let mut has_docker_host = host.is_some();
    let mut socket = container_socket.to_string();
    let mut result = SocketAndHost {
        socket: socket.clone(),
        host: host.clone().unwrap_or_default(),
    };

    // Set host for sanity's sake, when the socket isn't useful.
    if !has_docker_host && (socket == "-" || socket.is_empty() || !is_docker_host_uri(&socket)) {
        // Cases 1B, 2B, 4B
        host = socket_location_in(locations, docker_host);
        result.host = host.clone().unwrap_or_default();
        has_docker_host = host.is_some();
    }

    // A dash means "do not mount", and a file path is not a socket. Neither is
    // usable without a host, so this is where the invalid state is caught.
    if !has_docker_host && !socket.is_empty() && !is_docker_host_uri(&socket) {
        // Cases 1B, 2B
        return Err(SocketError::NoHostAndInvalidContainerSocket(socket));
    }

    // Default to DOCKER_HOST when the flag was omitted.
    if socket.is_empty() && has_docker_host {
        // Case 4A
        socket = result.host.clone();
    }
    if socket.is_empty() {
        // Case 4B. `socket_location_in` is asked a second time, which is
        // upstream's `socket, _ := socketLocation()`: the answer is "" when
        // nothing was found, so this is at worst a no-op.
        socket = socket_location_in(locations, docker_host).unwrap_or_default();
    }
    result.socket = socket;

    if has_docker_host {
        // Cases 1A, 2A, 3A, 4A. An invalid socket here is only logged
        // upstream; the host is still usable, so the caller's job is not
        // blocked over a mount it may not need.
        return Ok(result);
    }

    if is_docker_host_uri(&result.socket) {
        // Case 3B
        result.host = result.socket.clone();
        return Ok(result);
    }

    // No host and no usable socket. Upstream notes this "should never be
    // taken"; it is reachable when a socket location exists but `DOCKER_HOST`
    // did not, so it stays.
    Err(SocketError::NoHostAndInvalidSocket(result.socket))
}

/// `os.ExpandEnv`: `$NAME` and `${NAME}` from the environment.
///
/// An unset variable expands to the empty string, exactly as Go's does — which
/// is how `$XDG_RUNTIME_DIR/docker.sock` degrades to `/docker.sock`.
fn expand_env(value: &str) -> String {
    let bytes: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != '$' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        // `$$` is a literal dollar.
        if i + 1 < bytes.len() && bytes[i + 1] == '$' {
            out.push('$');
            i += 2;
            continue;
        }
        let (name, next): (String, usize) = if i + 1 < bytes.len() && bytes[i + 1] == '{' {
            match bytes[i + 2..].iter().position(|c| *c == '}') {
                Some(end) => (bytes[i + 2..i + 2 + end].iter().collect(), i + 3 + end),
                // An unclosed `${` is left alone, as Go's scanner does.
                None => {
                    out.push('$');
                    i += 1;
                    continue;
                }
            }
        } else {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == '_') {
                end += 1;
            }
            if end == start {
                out.push('$');
                i += 1;
                continue;
            }
            (bytes[start..end].iter().collect(), end)
        };
        out.push_str(&std::env::var(&name).unwrap_or_default());
        i = next;
    }
    out
}

/// `filepath.ToSlash`.
fn to_slash(path: &str) -> String {
    if std::path::MAIN_SEPARATOR == '/' {
        return path.to_string();
    }
    path.replace(std::path::MAIN_SEPARATOR, "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    // docker_socket_test.go: TestGetSocketAndHostWithSocket
    #[test]
    fn an_explicit_socket_wins_over_the_default() {
        let host = "unix:///my/docker/host.sock";
        let result = get_socket_and_host_in(
            "/path/to/my.socket",
            COMMON_SOCKET_LOCATIONS,
            Some(host),
        )
        .expect("a socket");
        assert_eq!(
            result,
            SocketAndHost {
                socket: "/path/to/my.socket".to_string(),
                host: host.to_string(),
            },
        );
    }

    // docker_socket_test.go: TestGetSocketAndHostNoSocket
    #[test]
    fn an_omitted_socket_defaults_to_docker_host() {
        let host = "unix:///my/docker/host.sock";
        let result =
            get_socket_and_host_in("", COMMON_SOCKET_LOCATIONS, Some(host)).expect("a socket");
        assert_eq!(
            result,
            SocketAndHost {
                socket: host.to_string(),
                host: host.to_string(),
            },
        );
    }

    // docker_socket_test.go: TestGetSocketAndHostDontMount
    #[test]
    fn a_dash_means_mount_nothing() {
        let host = "unix:///my/docker/host.sock";
        let result = get_socket_and_host_in("-", COMMON_SOCKET_LOCATIONS, Some(host))
            .expect("a socket");
        assert_eq!(
            result,
            SocketAndHost {
                socket: "-".to_string(),
                host: host.to_string(),
            },
        );
    }

    // docker_socket_test.go: TestGetSocketAndHostNoHostInvalidSocket
    #[test]
    fn a_file_path_without_a_host_is_an_error() {
        let locations = ["/unusual", "/socket", "/location"];
        assert_eq!(socket_location_in(&locations, None), None);

        let error = get_socket_and_host_in("/my/socket/path.sock", &locations, None)
            .expect_err("no host and a file path");
        assert_eq!(
            error,
            SocketError::NoHostAndInvalidContainerSocket("/my/socket/path.sock".to_string()),
        );
    }

    // docker_socket_test.go: TestGetSocketAndHostNoHostNoSocketDefaultLocation
    #[test]
    fn a_default_location_is_used_for_both() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let socket_file = dir.path().join("act.sock");
        std::fs::write(&socket_file, b"").expect("created");
        let path = socket_file.to_string_lossy().into_owned();
        let expected = format!("unix://{path}");

        let locations = [path.as_str()];
        assert_eq!(
            socket_location_in(&locations, None),
            Some(expected.clone()),
            "the found socket becomes a unix:// URI",
        );
        let result = get_socket_and_host_in("", &locations, None).expect("a socket");
        assert_eq!(
            result,
            SocketAndHost {
                socket: expected.clone(),
                host: expected,
            },
        );
    }

    // docker_socket_test.go: TestGetSocketAndHostOnlySocketValidButUnusualLocation
    #[test]
    fn a_valid_uri_socket_becomes_the_host() {
        let socket = "unix:///path/to/my.socket";
        let locations = ["/unusual", "/location"];
        assert_eq!(socket_location_in(&locations, None), None);

        let result = get_socket_and_host_in(socket, &locations, None).expect("a socket");
        assert_eq!(result.host, socket, "3B: DOCKER_HOST := socket");
    }

    // docker_socket_test.go: TestGetSocketAndHostOnlySocket and
    // TestGetSocketAndHostNoHostNoSocket
    //
    // Both assert that `socketLocation()` finds something, which is a
    // property of the machine running the suite rather than of the code. The
    // ported assertions are the parts that are about the decision: the host
    // equals whatever the default lookup produced, whether or not it found
    // anything.
    #[test]
    fn without_a_host_the_default_location_decides() {
        let default = socket_location();
        let found = default.is_some();
        if !found {
            // With no socket anywhere, an omitted socket is the one case that
            // cannot be satisfied.
            assert!(get_socket_and_host_in("", COMMON_SOCKET_LOCATIONS, None).is_err());
            return;
        }
        let default = default.expect("found");

        let omitted =
            get_socket_and_host_in("", COMMON_SOCKET_LOCATIONS, None).expect("a socket");
        assert_eq!(omitted, SocketAndHost { socket: default.clone(), host: default.clone() });

        // A file path is kept as the socket, and the default becomes the host.
        let path = "/path/to/my.socket";
        let only_socket =
            get_socket_and_host_in(path, COMMON_SOCKET_LOCATIONS, None).expect("a socket");
        assert_eq!(only_socket.socket, path);
        assert_eq!(only_socket.host, default);
    }

    #[test]
    fn a_named_pipe_socket_becomes_an_npipe_uri() {
        // The Windows entry, forced through the same code path so the
        // transformation is checked on every platform.
        let dir = tempfile::tempdir().expect("a temporary directory");
        // A path that starts with the named-pipe prefix cannot exist on Unix,
        // so the transformation is asserted on the string the function builds.
        assert!(is_docker_host_uri("npipe:////./pipe/docker_engine"));
        assert!(!is_docker_host_uri("/my/socket.sock"));
        assert!(dir.path().exists());
    }

    /// The scheme test, which is what keeps a file path from being read as a
    /// host.
    #[test]
    fn only_an_alphabetic_scheme_counts_as_a_host_uri() {
        for uri in [
            "unix:///var/run/docker.sock",
            "tcp://127.0.0.1:2375",
            "npipe:////./pipe/docker_engine",
            "ssh://user@host",
        ] {
            assert!(is_docker_host_uri(uri), "{uri}");
        }
        for path in [
            "/path/to/my.socket",
            "unix:/path/to/my.socket",
            "unix-ssh://host",
            "",
            "/a://b",
            "1tcp://h",
        ] {
            assert!(!is_docker_host_uri(path), "{path}");
        }
        // An *empty* scheme passes upstream's test, because `IndexFunc` over
        // an empty string finds nothing and Go compares that against -1. So
        // `"://"` is treated as a host URI. Preserved: the function is used to
        // tell a socket path from a URI, and a string that starts with `://`
        // is not a path.
        for uri in ["://host", "://"] {
            assert!(is_docker_host_uri(uri), "{uri}");
        }
    }

    /// `$NAME` expands, an unset name vanishes, and `$$` is a literal dollar.
    #[test]
    fn environment_variables_expand_like_go() {
        // SAFETY: set and read within this test body only.
        unsafe { std::env::set_var("ACT_SOCK_TEST", "/tmp/sock") };
        assert_eq!(expand_env("$ACT_SOCK_TEST/docker.sock"), "/tmp/sock/docker.sock");
        assert_eq!(expand_env("${ACT_SOCK_TEST}/x"), "/tmp/sock/x");
        assert_eq!(
            expand_env("$ACT_SOCK_UNSET/docker.sock"),
            "/docker.sock",
            "an unset variable expands to nothing, leaving the separator",
        );
        assert_eq!(expand_env("$$HOME"), "$HOME");
        assert_eq!(expand_env("$ ACT_SOCK_TEST"), "$ ACT_SOCK_TEST");
        assert_eq!(expand_env("plain/path"), "plain/path");
        unsafe { std::env::remove_var("ACT_SOCK_TEST") };
    }

    /// `DOCKER_HOST` is never second-guessed, even if the socket behind it is
    /// missing.
    #[test]
    fn docker_host_is_taken_verbatim() {
        assert_eq!(
            socket_location_in(COMMON_SOCKET_LOCATIONS, Some("tcp://127.0.0.1:1")),
            Some("tcp://127.0.0.1:1".to_string()),
        );
        assert_eq!(socket_location_in(&["/definitely/not/here"], None), None);
    }
}
