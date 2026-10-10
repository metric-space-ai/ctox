//! A minimal blocking HTTP/1.1 exchange, shared by the two services act runs.
//!
//! Both [`crate::artifactcache`] and [`crate::artifacts`] are small JSON/file
//! services with a handful of fixed routes. Pulling in a server framework for
//! them would add more machinery than the two services themselves are worth,
//! so the request parsing, the response writing and the target splitting live
//! here and each service contributes only its routes and handlers.
//!
//! Two shapes are offered, and the tests use both:
//!
//! * [`serve_connection`] for the real thing, over a socket. act's own tests
//!   drive `net/http` over a real listener, so the ported tests do too.
//! * [`Call`] plus a handler, for routing that can be exercised without a
//!   socket. A test double and the real implementation are two different
//!   specifications, and a service is worth checking against both.
//!
//! Deviations from `net/http`:
//!
//! * Every response closes the connection, so keep-alive is not offered and
//!   the client's next request opens a new one. Neither service is
//!   request-rate-limited in a way that makes reuse matter.
//! * The read deadline covers the whole request, not only the header. act
//!   configures `ReadHeaderTimeout: 2s`; the body follows immediately in both
//!   services, so a single deadline is the same constraint in practice.
//! * There is no chunked request decoding. Neither service's clients
//!   (`actions/cache`, `actions/upload-artifact`, `upload-artifact@v4`) send a
//!   chunked body, and act's `net/http` is the one that would have had to
//!   accept it.

use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// act's `ReadHeaderTimeout`.
pub const READ_HEADER_TIMEOUT: Duration = Duration::from_secs(2);

/// A response, held separately from the transport so routing can be tested
/// without a socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// The status code.
    pub status: u16,
    /// The response body.
    pub body: Vec<u8>,
    /// `Content-Type`. act's own replies carry JSON; the artifact download
    /// carries the archive and lets the caller sniff it.
    pub content_type: &'static str,
    /// Extra headers, in the order they are written. Only
    /// `Content-Encoding: gzip` is needed, and only by the artifact server:
    /// it is how a stored `.gz__` file is marked for the client to inflate.
    pub headers: Vec<(String, String)>,
}

impl Reply {
    /// A reply with a body and a content type.
    pub fn new(status: u16, content_type: &'static str, body: Vec<u8>) -> Self {
        Reply {
            status,
            body,
            content_type,
            headers: Vec::new(),
        }
    }

    /// Adds a header, in the order it will be written.
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// The first header named `name`, compared case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// A JSON reply.
    pub fn json(status: u16, value: serde_json::Value) -> Self {
        Reply {
            status,
            body: serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec()),
            content_type: "application/json; charset=utf-8",
            headers: Vec::new(),
        }
    }

    /// A JSON reply whose body is the empty object, which is what act's
    /// `responseJSON` writes when it is called without a value.
    pub fn empty_json(status: u16) -> Self {
        Reply {
            status,
            body: b"{}".to_vec(),
            content_type: "application/json; charset=utf-8",
            headers: Vec::new(),
        }
    }

    /// A JSON reply with act's V4 content type, which has no space after the
    /// semicolon. The artifact client checks it.
    pub fn protojson(status: u16, value: serde_json::Value) -> Self {
        Reply {
            status,
            body: serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec()),
            content_type: "application/json;charset=utf-8",
            headers: Vec::new(),
        }
    }

    /// A reply with no body, which is what act's V4 error paths answer.
    pub fn status(status: u16) -> Self {
        Reply {
            status,
            body: Vec::new(),
            content_type: "text/plain; charset=utf-8",
            headers: Vec::new(),
        }
    }

    /// A reply with no body that carries a `Content-Type`, which is what Go's
    /// sniffing settles on for a body act wrote without one.
    pub fn sniffed_empty(status: u16) -> Self {
        Reply {
            status,
            body: Vec::new(),
            content_type: SNIFFED,
            headers: Vec::new(),
        }
    }

    /// The body parsed as JSON.
    pub fn json_body(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

/// One parsed request.
#[derive(Debug, Clone, Default)]
pub struct Call {
    /// `GET`, `POST`, `PATCH`, `PUT`, ...
    pub method: String,
    /// The path, percent-decoded.
    pub path: String,
    /// The decoded query parameters, in order.
    pub query: Vec<(String, String)>,
    /// Request headers with lowercased names.
    pub headers: Vec<(String, String)>,
    /// The request body.
    pub body: Vec<u8>,
    /// The `Host` header, which act uses to build the URLs it hands back.
    pub host: String,
}

impl Call {
    /// A `GET` for `target`, which may carry a query string.
    pub fn get(target: &str) -> Self {
        let (path, query) = split_target(target);
        Call {
            method: "GET".to_string(),
            path,
            query,
            ..Call::default()
        }
    }

    /// A request with a method, a target and a body.
    pub fn new(method: &str, target: &str, body: &[u8]) -> Self {
        let (path, query) = split_target(target);
        Call {
            method: method.to_string(),
            path,
            query,
            headers: Vec::new(),
            body: body.to_vec(),
            host: String::new(),
        }
    }

    /// Adds a header.
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers
            .push((name.to_ascii_lowercase(), value.to_string()));
        self
    }

    /// Sets the `Host` header, which the artifact service reflects into the
    /// URLs it returns.
    pub fn with_host(mut self, host: &str) -> Self {
        self.host = host.to_string();
        self
    }

    /// The first value of `name` in the query string, or `""`.
    ///
    /// act reads `r.URL.Query().Get(name)`, which is exactly this: the first
    /// occurrence, empty when absent.
    pub fn query_get(&self, name: &str) -> &str {
        self.query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .unwrap_or("")
    }

    /// The first header named `name`, compared case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The path split into non-empty segments.
    pub fn segments(&self) -> Vec<&str> {
        self.path.split('/').filter(|s| !s.is_empty()).collect()
    }

    /// The body parsed as JSON.
    pub fn json_body(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }
}

/// A `Content-Type` of "none".
///
/// `net/http` only runs its sniffer when the handler actually writes a body,
/// so a handler that writes nothing sends no `Content-Type` at all. act's V4
/// upload reply is exactly that case.
pub const NO_CONTENT_TYPE: &str = "";

/// The `Content-Type` Go's `http.DetectContentType` settles on for act's JSON
/// bodies. act writes them with a bare `w.Write`, so there is no declared type
/// and the sniffer looks at the first 512 bytes: a body starting with `{` is
/// text, which is `text/plain; charset=utf-8`.
pub const SNIFFED: &str = "text/plain; charset=utf-8";

/// What a service does with a request: answer it, claim nothing, or fail.
///
/// The error is not a status code. act's artifact handlers `panic` on every
/// IO failure, and `net/http` recovers a panicking handler by closing the
/// connection without writing a response. So an error here closes the
/// connection and sends nothing, which is what act's callers observe. The
/// cache service has no panicking handler and never returns one.
pub trait Service: Send + Sync {
    /// Routes one request. `Ok(None)` means no route matched, which the server
    /// turns into a bare 404.
    fn route(&self, call: &Call) -> Result<Option<Reply>, io::Error>;
}

/// Reads one request off an accepted connection and writes the reply.
pub fn serve_connection(mut stream: TcpStream, service: &dyn Service) -> io::Result<()> {
    stream.set_read_timeout(Some(READ_HEADER_TIMEOUT))?;

    let mut request = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    let head_len = loop {
        if let Some(end) = find_header_end(&request) {
            break end;
        }
        let read = stream.read(&mut buf)?;
        if read == 0 {
            // The peer closed before the head was complete.
            return Ok(());
        }
        request.extend_from_slice(&buf[..read]);
    };

    let head = String::from_utf8_lossy(&request[..head_len]).into_owned();
    let mut lines = head.split("\r\n");
    let Some(request_line) = lines.next() else {
        return Ok(());
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
    let mut host = String::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            match name.as_str() {
                "content-length" => content_length = value.parse().unwrap_or(0),
                "host" => host = value.clone(),
                _ => {}
            }
            headers.push((name, value));
        }
    }

    let body_start = head_len + 4;
    while request.len() < body_start + content_length {
        let read = stream.read(&mut buf)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buf[..read]);
    }
    let body = request
        .get(body_start..body_start + content_length)
        .unwrap_or_default()
        .to_vec();

    let (path, query) = split_target(&target);
    let call = Call {
        method,
        path,
        query,
        headers,
        body,
        host,
    };
    let Some(reply) = service.route(&call)? else {
        return write_reply(&mut stream, Reply::status(404));
    };
    write_reply(&mut stream, reply)
}

/// Writes one response, extra headers first so `Content-Type` wins the
/// position act's own replies use.
fn write_reply(stream: &mut TcpStream, reply: Reply) -> io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\n",
        reply.status,
        reason_phrase(reply.status),
        reply.body.len(),
    );
    if !reply.content_type.is_empty() {
        head.push_str("Content-Type: ");
        head.push_str(reply.content_type);
        head.push_str("\r\n");
    }
    for (name, value) in &reply.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("Connection: close\r\n\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(&reply.body)?;
    stream.flush()
}

/// The offset of the blank line ending the request head.
pub fn find_header_end(request: &[u8]) -> Option<usize> {
    request.windows(4).position(|window| window == b"\r\n\r\n")
}

/// Splits `/path?a=b&c=d` into its decoded path and query pairs.
///
/// An absolute-form target (`http://host/path`, which RFC 7230 allows and
/// which act's own signed URLs are written as) has its scheme and authority
/// dropped first, so that `r.URL.Path` means the same thing here as it does in
/// `net/http`.
pub fn split_target(target: &str) -> (String, Vec<(String, String)>) {
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target, None),
    };
    let path = strip_authority(path);
    let query = query
        .map(|query| {
            query
                .split('&')
                .filter(|pair| !pair.is_empty())
                .map(|pair| match pair.split_once('=') {
                    Some((key, value)) => (percent_decode(key), percent_decode(value)),
                    None => (percent_decode(pair), String::new()),
                })
                .collect()
        })
        .unwrap_or_default();
    (percent_decode(path), query)
}

/// Drops the scheme and authority of an absolute-form request target.
fn strip_authority(path: &str) -> &str {
    for prefix in ["http://", "https://"] {
        if let Some(rest) = path.strip_prefix(prefix) {
            // A target with no path at all keeps its `/`.
            return match rest.find('/') {
                Some(index) => &rest[index..],
                None => "/",
            };
        }
    }
    path
}

/// `url.QueryUnescape` for the characters a cache key, artifact name or route
/// segment can contain. A `+` in a query value is a space, as in Go; in a path
/// it is a literal plus, which is also what Go does.
pub fn percent_decode(value: &str) -> String {
    let in_path = value.starts_with('/');
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&value[i + 1..i + 3], 16) {
                Ok(byte) => {
                    out.push(byte);
                    i += 3;
                }
                Err(_) => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            b'+' if !in_path => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `url.QueryEscape`: percent-encoding, with a space as `+`.
pub fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            byte => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The reason phrase for a status code, so the wire response reads normally.
pub fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_split_into_path_and_query() {
        let (path, query) = split_target("/a/b?x=1&y=two%20words&z");
        assert_eq!(path, "/a/b");
        assert_eq!(query[0], ("x".to_string(), "1".to_string()));
        assert_eq!(query[1], ("y".to_string(), "two words".to_string()));
        assert_eq!(query[2], ("z".to_string(), String::new()));
    }

    #[test]
    fn a_plus_is_a_space_in_a_query_but_not_in_a_path() {
        let (path, query) = split_target("/a+b?x=1+2");
        assert_eq!(path, "/a+b");
        assert_eq!(query[0], ("x".to_string(), "1 2".to_string()));
    }

    #[test]
    fn percent_encoding_escapes_the_query_specials() {
        assert_eq!(percent_encode("a b"), "a+b");
        assert_eq!(percent_encode("a/b"), "a%2Fb");
        assert_eq!(percent_encode("a&b=c"), "a%26b%3Dc");
        assert_eq!(
            percent_encode("2024-01-02 03:04:05.9 +0200 CEST"),
            "2024-01-02+03%3A04%3A05.9+%2B0200+CEST"
        );
        // Round trip.
        let encoded = percent_encode("a b/c&d=e%f");
        assert_eq!(
            split_target(&format!("/p?k={encoded}")).1[0].1,
            "a b/c&d=e%f"
        );
    }

    #[test]
    fn the_first_query_value_wins() {
        let call = Call::get("/p?k=first&k=second");
        assert_eq!(call.query_get("k"), "first");
        assert_eq!(Call::get("/p").query_get("k"), "");
    }

    #[test]
    fn an_absolute_form_target_keeps_only_its_path() {
        let (path, query) = split_target("http://localhost:12345/upload/1?itemPath=some/file");
        assert_eq!(path, "/upload/1");
        assert_eq!(query[0], ("itemPath".to_string(), "some/file".to_string()));
        assert_eq!(split_target("https://example.com").0, "/");
        // An origin-form target is untouched.
        assert_eq!(split_target("/upload/1?itemPath=a").0, "/upload/1");
    }

    #[test]
    fn segments_skip_the_empty_parts() {
        assert_eq!(Call::get("/a//b/").segments(), ["a", "b"]);
    }
}
