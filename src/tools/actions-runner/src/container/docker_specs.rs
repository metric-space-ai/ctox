//! The four little grammars act's `parse()` borrows to read a `-p`, `--expose`,
//! `-v` and `--device` value.
//!
//! act's `pkg/container/docker_cli.go` is itself an adaptation of
//! `docker/cli`'s `cli/command/container/opts.go`, and the parts of it that
//! decide *what a flag means* are in turn adaptations of four upstream
//! packages, all of which are pure string parsing with no daemon, no socket and
//! no filesystem. That is the whole reason this module can be tested at all:
//! every rule below is exercised by upstream's own table tests, brought across.
//!
//! * [`nat`] — `github.com/docker/go-connections@v0.6.0/nat`: the short
//!   publish syntax `[ip:]public:private[/proto]`. `parse()` calls
//!   [`nat::parse_port_specs`] on the output of
//!   [`super::docker_opts::convert_to_standard_notation`], which lives in the
//!   sibling module and is deliberately **not** duplicated here.
//! * [`network`] — `github.com/moby/moby/api/types/network`: `parse()` runs
//!   [`network::parse_port`] over each already-converted `nat` port and
//!   [`network::parse_port_range`] over each `--expose` value.
//! * [`volume`] — `docker/cli`'s `internal/volumespec`, reached through
//!   `loader.ParseVolume`, which since v28 is a one-line shim over it.
//! * [`cdi`] — `tags.cncf.io/container-device-interface/pkg/parser`, which
//!   decides whether `--device` names a host device path or a CDI qualified
//!   name.
//!
//! The names are kept as upstream spells them, in nested modules, because
//! `parse()`'s two port parsers are *different functions with different
//! semantics* on the same input. `nat.ParsePort` returns an `int` and wraps
//! `strconv`'s error as `invalid port 'x': invalid syntax`;
//! `network.ParsePort` returns a validated `Port` and says
//! `invalid port: value is empty`. Flattening them into one namespace would
//! force a rename that hides which one `parse()` is calling.
//!
//! # The error text is the contract
//!
//! Upstream asserts on these strings verbatim, and they reach the user: a
//! workflow's `options:` with a bad `-p` prints
//! `invalid range format for --expose: invalid start port 'NaN': invalid
//! syntax`. They are reproduced exactly, including Go's `strconv` wording
//! (`strconv.ParseUint: parsing "asdf": invalid syntax`), which is why the
//! shims below exist at all: a Rust `ParseIntError` says something else.
//!
//! # Deliberate deviation: CDI does not panic on a one-character vendor
//!
//! `parser.validateVendorOrClassName` iterates `name[1 : len(name)-1]`, which
//! is `name[1:0]` for a one-character name — an out-of-range slice. Go panics,
//! and so `cdi.IsQualifiedName("vendor.com/c=d")` panics upstream, taking the
//! whole runner down over one `--device` flag; a one-character *class* panics
//! the same way. This port clamps the slice to empty instead, which is what the
//! surrounding checks evidently meant: `c` is a legal class, because it starts
//! with a letter and ends with an alphanumeric. So [`cdi::is_qualified_name`]
//! answers `true` where upstream would crash, and reports the ordinary
//! "should start with letter" error for a one-character name that begins with
//! punctuation. The alternative — a faithful panic — was rejected because act
//! runs on a build agent, where a malformed flag must not be able to abort the
//! job. Every other quirk **is** preserved, including the ones that look like
//! bugs, and each says which upstream file it came from.
//!
//! # `net.ParseIP` and `strconv.ParseUint` are ports, not calls
//!
//! Neither has a Rust equivalent with the same accept/reject behaviour, and
//! both are load-bearing: a leading `+` is a syntax error to `ParseUint`, `01`
//! is not an IP address, and `[]` is a *valid* bracketed IPv6 with an empty
//! host. [`go_parse_uint`], [`go_parse_ip`], [`go_split_host_port`] and
//! [`go_join_host_port`] are the small stdlib pieces the four grammars need,
//! written against the behaviour verified with go1.26.2.
//!
//! # What is deliberately not here
//!
//! * `loader/windows_path.go` (`isAbs`, `volumeNameLen`). It is not on
//!   `ParseVolume`'s path — its one caller is the *secret and config file*
//!   loader, which act never reaches. The Windows handling that `-v` actually
//!   observes lives in `volumespec.go` and is ported: the single-letter drive
//!   `C:` that makes `C:\path` one field rather than two
//!   ([`volume::is_windows_drive`]), and the `\\` named-pipe prefix in
//!   [`volume::is_file_path`].
//! * `volumespec`'s `Image`, `Tmpfs` and `Cluster` option structs, and
//!   `Consistency`. `Parse` can never populate them, so porting them would add
//!   states Go cannot produce. [`volume::MountOpts`] keeps the two families
//!   `Parse` does emit as an enum for the same reason.
//! * `network.Port`'s `MarshalText`/`UnmarshalText`/`AppendTo` and the JSON
//!   round-trips in its test table. Those assert Go's `encoding` plumbing, not
//!   the grammar; `String` is ported and the table's identity
//!   (`ParsePort(port.String()) == port`) still runs.
//! * `MustParsePort`/`MustParsePortRange`, which panic. Rust's `expect` is the
//!   same thing under a different name, so the tests call that instead.

use std::net::IpAddr;

// ---------------------------------------------------------------------------
// Go stdlib shims
// ---------------------------------------------------------------------------

const ERR_SYNTAX: &str = "invalid syntax";
const ERR_RANGE: &str = "value out of range";

/// Go's `strconv.Quote` for a `string`.
///
/// The `strconv` errors below interpolate their input with `%q`, so the
/// quoting is part of the message a user sees. Non-ASCII is passed through,
/// which is what Go does for printable runes; a `str` is valid UTF-8 in Rust,
/// so Go's invalid-byte case cannot arise.
fn go_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{b}' => out.push_str("\\v"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `strconv.ParseUint(s, 10, bit_size)`, returning Go's bare failure reason.
///
/// Only ASCII digits are accepted, so `+1`, `0x10`, `1_2`, ` 1` and a leading
/// sign are all [`ERR_SYNTAX`] — `ParseUint` with an explicit base of 10 does
/// not permit a sign prefix. Leading zeros are ordinary decimal, so `0123` is
/// 123; go-connections carries a `FIXME` about that and it is preserved.
fn go_parse_uint(s: &str, bit_size: u32) -> Result<u64, &'static str> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ERR_SYNTAX);
    }
    let limit = 1u64 << bit_size;
    let mut acc = 0u64;
    for b in s.bytes() {
        // The bound is re-checked every digit, so `acc` cannot overflow: it is
        // abandoned the moment it reaches `limit`.
        acc = acc * 10 + u64::from(b - b'0');
        if acc >= limit {
            return Err(ERR_RANGE);
        }
    }
    Ok(acc)
}

/// The `*strconv.NumError` as `nat.ParsePortRange` returns it — unrewrapped.
fn go_parse_uint_error(s: &str, reason: &str) -> String {
    format!("strconv.ParseUint: parsing {}: {reason}", go_quote(s))
}

/// `net.ParseIP`: a predicate, never a value source.
///
/// `ParsePortSpec` keeps the *original* string and uses this only to ask
/// whether it is an address, so a disagreement about the parsed form — Go
/// reports `::ffff:1.2.3.4` as an IPv4 address, Rust as an IPv6 one — cannot
/// change any output. What does matter, and agrees: no leading zeros in a
/// dotted quad, no surrounding whitespace, and no zone suffix.
fn go_parse_ip(s: &str) -> Option<IpAddr> {
    s.parse::<IpAddr>().ok()
}

/// `net.SplitHostPort`, whose error is an `*net.AddrError` reading
/// `address <hostport>: <reason>`.
fn go_split_host_port(host_port: &str) -> Result<(String, String), String> {
    const MISSING_PORT: &str = "missing port in address";
    const TOO_MANY_COLONS: &str = "too many colons in address";
    let bytes = host_port.as_bytes();
    let addr_err = |why: &str| format!("address {host_port}: {why}");

    let Some(i) = host_port.rfind(':') else {
        return Err(addr_err(MISSING_PORT));
    };
    let (host, j, k) = if bytes.first() == Some(&b'[') {
        // A bracketed host must close just before the final colon.
        let Some(end) = host_port.find(']') else {
            return Err(addr_err("missing ']' in address"));
        };
        match end + 1 {
            len if len == host_port.len() => return Err(addr_err(MISSING_PORT)),
            len if len == i => (host_port[1..end].to_string(), 1, end + 1),
            _ if bytes[end + 1] == b':' => return Err(addr_err(TOO_MANY_COLONS)),
            _ => return Err(addr_err(MISSING_PORT)),
        }
    } else {
        let host = host_port[..i].to_string();
        if host.contains(':') {
            return Err(addr_err(TOO_MANY_COLONS));
        }
        (host, 0, 0)
    };
    if host_port[j..].contains('[') {
        return Err(addr_err("unexpected '[' in address"));
    }
    if host_port[k..].contains(']') {
        return Err(addr_err("unexpected ']' in address"));
    }
    Ok((host, host_port[i + 1..].to_string()))
}

/// `net.JoinHostPort`: bracket the host only when it contains a colon, so an
/// *empty* host is left unbracketed. That is what makes `nat`'s
/// `PortMapping.String` print `::6000/tcp` for a spec with no host at all.
fn go_join_host_port(host: &str, port: &str) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

// ---------------------------------------------------------------------------
// github.com/docker/go-connections/nat
// ---------------------------------------------------------------------------

/// The short publish syntax, as act's `parse()` uses it.
///
/// Split out from [`docker_opts`] because the two are called in sequence:
/// `convert_to_standard_notation` turns `-p published=80,target=8080` into
/// `80:8080/tcp`, and *then* this grammar reads it.
pub mod nat {
    use super::{
        go_join_host_port, go_parse_ip, go_parse_uint, go_parse_uint_error, go_split_host_port,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::fmt;

    /// `nat.Port`: a port number and protocol, `"80/tcp"`.
    ///
    /// Upstream's is a bare `string`, so a `Port` here can hold anything —
    /// including a range (`"1234-1242/tcp"`), which `NewPort` produces and
    /// `Range` reads back. The port *set* a `parse()` hands to the daemon only
    /// ever holds single ports, because `ParsePortSpec` expands a range into
    /// one `Port` per number.
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Port(String);

    impl Port {
        /// The `"80/tcp"` spelling, which is also the map key upstream.
        pub fn as_str(&self) -> &str {
            &self.0
        }

        /// `p.Proto()`.
        pub fn proto(&self) -> String {
            split_proto_port(&self.0).proto
        }

        /// `p.Port()` — the number or range, without the protocol.
        pub fn port(&self) -> String {
            split_proto_port(&self.0).port
        }

        /// `p.Int()`: the number, or `0`.
        ///
        /// Upstream deliberately drops the error, on the grounds that a `Port`
        /// that failed to parse could not have been constructed. A `Port` here
        /// can be built from a string, so the same assumption is not free —
        /// but the only way to get here is through [`parse_port_spec`], which
        /// has already validated the number.
        pub fn int(&self) -> u16 {
            parse_port(&self.port()).unwrap_or(0)
        }

        /// `p.Range()`: the start and end of the port or range.
        pub fn range(&self) -> Result<(u16, u16), String> {
            parse_port_range_to_int(&self.port())
        }
    }

    impl fmt::Display for Port {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.0)
        }
    }

    /// `nat.PortBinding`: where on the host a published port is bound.
    ///
    /// Both fields are empty strings rather than `Option`s when unset, because
    /// that is what upstream stores and what a `HostConfig` serialises; an
    /// absent host IP and an empty one are the same request to the daemon.
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct PortBinding {
        /// The host address to bind, empty for every address.
        pub host_ip: String,
        /// The host port, or a range for a dynamic allocation.
        pub host_port: String,
    }

    /// `nat.PortSet`: the exposed ports, as a set.
    pub type PortSet = BTreeSet<Port>;

    /// `nat.PortMap`: the bindings, per exposed port.
    pub type PortMap = BTreeMap<Port, Vec<PortBinding>>;

    /// `nat.PortMapping`: one expanded port and where it is published.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PortMapping {
        /// The container port, protocol included.
        pub port: Port,
        /// Where it is published on the host.
        pub binding: PortBinding,
    }

    impl fmt::Display for PortMapping {
        /// `(*PortMapping).String()`. Note the port is appended *inside* the
        /// host-port field, so a mapping reads `[::1]:8080:6000/tcp`.
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&go_join_host_port(
                &self.binding.host_ip,
                &format!("{}:{}", self.binding.host_port, self.port),
            ))
        }
    }

    /// The two halves of a `<portnum>/[<proto>]` spec.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ProtoPort {
        /// The protocol, defaulting to `tcp`.
        pub proto: String,
        /// The port or range.
        pub port: String,
    }

    /// `nat.SplitProtoPort`.
    ///
    /// Returns two empty strings when there is no port at all, and does
    /// **not** validate or normalise: `"1234/UDP"` comes back as `UDP`, and
    /// `"any port value"` is happily a port.
    pub fn split_proto_port(raw_port: &str) -> ProtoPort {
        let (port, proto) = match raw_port.split_once('/') {
            Some((port, proto)) => (port, proto),
            None => (raw_port, ""),
        };
        if port.is_empty() {
            return ProtoPort {
                proto: String::new(),
                port: String::new(),
            };
        }
        ProtoPort {
            proto: if proto.is_empty() { "tcp" } else { proto }.to_string(),
            port: port.to_string(),
        }
    }

    /// `nat.ParsePort`: one port number as an `int`.
    ///
    /// The empty string is `0` with no error, which is how `Port.Int()` can
    /// be total. Upstream *unwraps* `strconv`'s error, so the message is
    /// `invalid port 'x': invalid syntax` — the `strconv.ParseUint: parsing`
    /// prefix is dropped here and kept in [`parse_port_range`], exactly as the
    /// two callers differ.
    pub fn parse_port(raw_port: &str) -> Result<u16, String> {
        if raw_port.is_empty() {
            return Ok(0);
        }
        go_parse_uint(raw_port, 16)
            .map(|p| p as u16)
            .map_err(|reason| format!("invalid port '{raw_port}': {reason}"))
    }

    /// `nat.ParsePortRange`: `8000-9000` or a single number, as start and end.
    ///
    /// A range with three or more numbers is *not* an error here: `1-2-3`
    /// parses `1` and `2` and ignores the rest, because only `parts[0]` and
    /// `parts[1]` are read. An empty string is an error, unlike in
    /// [`parse_port`].
    pub fn parse_port_range(ports: &str) -> Result<(u16, u16), String> {
        if ports.is_empty() {
            return Err("empty string specified for ports".to_string());
        }
        if !ports.contains('-') {
            let start = go_parse_uint(ports, 16).map_err(|r| go_parse_uint_error(ports, r))?;
            return Ok((start as u16, start as u16));
        }
        let mut parts = ports.split('-');
        let start_str = parts.next().unwrap_or_default();
        let end_str = parts.next().unwrap_or_default();
        let start =
            go_parse_uint(start_str, 16).map_err(|r| go_parse_uint_error(start_str, r))? as u16;
        let end = go_parse_uint(end_str, 16).map_err(|r| go_parse_uint_error(end_str, r))? as u16;
        if end < start {
            return Err(format!("invalid range specified for port: {ports}"));
        }
        Ok((start, end))
    }

    /// `nat.ParsePortRangeToInt`: [`parse_port_range`] that accepts an empty
    /// string, yielding `(0, 0)`.
    pub fn parse_port_range_to_int(raw_port: &str) -> Result<(u16, u16), String> {
        if raw_port.is_empty() {
            return Ok((0, 0));
        }
        parse_port_range(raw_port)
    }

    /// `nat.NewPort`: build a `Port` from a protocol and a number or range.
    ///
    /// The range is *validated* here and then re-rendered, which is why
    /// `NewPort("tcp", "1234")` cannot produce a non-canonical `0123/tcp`.
    pub fn new_port(proto: &str, port: &str) -> Result<Port, String> {
        let (start, end) = parse_port_range_to_int(port)?;
        if start == end {
            return Ok(Port(format!("{start}/{proto}")));
        }
        Ok(Port(format!("{start}-{end}/{proto}")))
    }

    /// `validateProto`: only these three, and the message is the concatenation.
    fn validate_proto(proto: &str) -> Result<(), String> {
        match proto {
            "tcp" | "udp" | "sctp" => Ok(()),
            other => Err(format!("invalid proto: {other}")),
        }
    }

    /// `splitParts`: the tail is the container port, the pair before it is the
    /// host port, and **everything** before that is the host address. A bare
    /// IPv6 address has no colons of its own to hide behind, so
    /// `2001:4860:0:2001::68::333` only works because the address is
    /// re-joined from `parts[:n-2]`.
    fn split_parts(raw_port: &str) -> (String, String, String) {
        let parts: Vec<&str> = raw_port.split(':').collect();
        match parts.len() {
            1 => (String::new(), String::new(), parts[0].to_string()),
            2 => (String::new(), parts[0].to_string(), parts[1].to_string()),
            3 => (
                parts[0].to_string(),
                parts[1].to_string(),
                parts[2].to_string(),
            ),
            n => {
                let (host, tail) = parts.split_at(n - 2);
                (host.join(":"), tail[0].to_string(), tail[1].to_string())
            }
        }
    }

    /// `nat.ParsePortSpec`: one `[ip:]public:private[/proto]` spec, expanded.
    ///
    /// # The order of the checks is the behaviour
    ///
    /// The protocol is validated *before* the address and *before* the ports
    /// are looked at, so `80:` — whose container port is empty — fails with
    /// `invalid proto: ` rather than the `no port specified` message that reads
    /// as though it should apply. And the two errors that mention a port
    /// discard the underlying `strconv` message, because upstream builds them
    /// with `errors.New` and concatenation rather than `%w`.
    pub fn parse_port_spec(raw_port: &str) -> Result<Vec<PortMapping>, String> {
        let (ip, host_port, container_port_raw) = split_parts(raw_port);
        let split = split_proto_port(&container_port_raw);
        let proto = split.proto.to_lowercase();
        validate_proto(&proto)?;

        // Brackets exist so an IPv6 address can survive `splitParts`; the
        // daemon wants the bare address.
        let mut unbracketed = None;
        if !ip.is_empty() && ip.starts_with('[') {
            match go_split_host_port(&format!("{ip}:")) {
                Ok((raw_ip, _)) => unbracketed = Some(raw_ip),
                Err(err) => return Err(format!("invalid IP address {ip}: {err}")),
            }
        }
        let ip: &str = match &unbracketed {
            Some(raw_ip) => raw_ip,
            None => ip.as_str(),
        };
        let container_port = split.port.as_str();
        if !ip.is_empty() && go_parse_ip(ip).is_none() {
            return Err(format!("invalid IP address: {ip}"));
        }
        if container_port.is_empty() {
            return Err(format!("no port specified: {raw_port}<empty>"));
        }

        let (start_port, end_port) = parse_port_range(container_port)
            .map_err(|_| format!("invalid containerPort: {container_port}"))?;

        let mut start_host_port = 0u16;
        let mut end_host_port = 0u16;
        if !host_port.is_empty() {
            let range = parse_port_range(&host_port)
                .map_err(|_| format!("invalid hostPort: {host_port}"))?;
            start_host_port = range.0;
            end_host_port = range.1;
            // A host range is allowed alongside a *single* container port, where
            // it is the daemon's dynamic allocation range. Alongside a
            // container range it is a genuine mistake.
            if (end_port - start_port) != (end_host_port - start_host_port)
                && end_port != start_port
            {
                return Err(format!(
                    "invalid ranges specified for container and host Ports: {container_port} and {host_port}"
                ));
            }
        }

        let count = (end_port - start_port) as u32 + 1;
        let mut ports = Vec::with_capacity(count as usize);
        for i in 0..count {
            let c_port = Port(format!("{}/{proto}", start_port as u32 + i));
            let mut h_port = String::new();
            if !host_port.is_empty() {
                h_port = (start_host_port as u32 + i).to_string();
                if count == 1 && start_host_port != end_host_port {
                    h_port += &format!("-{end_host_port}");
                }
            }
            ports.push(PortMapping {
                port: c_port,
                binding: PortBinding {
                    host_ip: ip.to_string(),
                    host_port: h_port,
                },
            });
        }
        Ok(ports)
    }

    /// `nat.ParsePortSpecs`: every spec, as an exposed set plus a binding map.
    ///
    /// The first spec that fails aborts the whole call and **discards** what
    /// came before, which is why act's `parse()` returns the error rather than
    /// a partial config.
    pub fn parse_port_specs(ports: &[String]) -> Result<(PortSet, PortMap), String> {
        let mut exposed_ports = PortSet::new();
        let mut bindings: PortMap = BTreeMap::new();
        for p in ports {
            let mappings = parse_port_spec(p)?;
            for pm in mappings {
                exposed_ports.insert(pm.port.clone());
                bindings.entry(pm.port).or_default().push(pm.binding);
            }
        }
        Ok((exposed_ports, bindings))
    }

    /// `nat`'s four tests, plus the Go stdlib edges `TestParsePortRange` and
    /// `TestSplitProtoPort` pin down.
    #[cfg(test)]
    mod tests {
        use super::*;

        // nat/nat_test.go: TestParsePort
        #[test]
        fn one_port_is_parsed_to_a_number() {
            struct Case {
                doc: &'static str,
                input: &'static str,
                exp_port: u16,
                exp_err: Option<&'static str>,
            }
            let cases = [
                Case {
                    doc: "invalid value",
                    input: "asdf",
                    exp_port: 0,
                    exp_err: Some("invalid port 'asdf': invalid syntax"),
                },
                Case {
                    doc: "invalid value with number",
                    input: "1asdf",
                    exp_port: 0,
                    exp_err: Some("invalid port '1asdf': invalid syntax"),
                },
                Case {
                    doc: "empty value",
                    input: "",
                    exp_port: 0,
                    exp_err: None,
                },
                Case {
                    doc: "zero value",
                    input: "0",
                    exp_port: 0,
                    exp_err: None,
                },
                Case {
                    doc: "negative value",
                    input: "-1",
                    exp_port: 0,
                    exp_err: Some("invalid port '-1': invalid syntax"),
                },
                // The FIXME upstream leaves on this case: `0123` is a valid
                // port. Base 10 is explicit, so it is not octal.
                Case {
                    doc: "octal value",
                    input: "0123",
                    exp_port: 123,
                    exp_err: None,
                },
                Case {
                    doc: "max value",
                    input: "65535",
                    exp_port: 65535,
                    exp_err: None,
                },
                Case {
                    doc: "value out of range",
                    input: "65536",
                    exp_port: 0,
                    exp_err: Some("invalid port '65536': value out of range"),
                },
            ];
            for tc in cases {
                match parse_port(tc.input) {
                    Ok(port) => {
                        assert!(tc.exp_err.is_none(), "{}: unexpected ok", tc.doc);
                        assert_eq!(port, tc.exp_port, "{}", tc.doc);
                    }
                    Err(err) => {
                        assert_eq!(Some(err.as_str()), tc.exp_err, "{}", tc.doc);
                        assert_eq!(0, tc.exp_port, "{}", tc.doc);
                    }
                }
            }
        }

        // nat/nat_test.go: TestParsePortRangeToInt
        #[test]
        fn the_int_wrapper_additionally_accepts_an_empty_range() {
            assert_eq!(parse_port_range_to_int("").expect("empty is fine"), (0, 0));
            assert_eq!(
                parse_port_range_to_int("8000-9000").expect("a range"),
                (8000, 9000)
            );
        }

        // nat/nat_test.go: TestPort
        #[test]
        fn a_new_port_renders_its_number_and_range() {
            let p = new_port("tcp", "1234").expect("1234");
            assert_eq!(p.as_str(), "1234/tcp");
            assert_eq!(p.proto(), "tcp");
            assert_eq!(p.port(), "1234");
            assert_eq!(p.int(), 1234);
            assert_eq!(p.range().expect("range"), (1234, 1234));

            assert!(new_port("tcp", "asd1234").is_err());
            assert!(new_port("tcp", "1234-1230").is_err());

            let p = new_port("tcp", "1234-1242").expect("a range");
            assert_eq!(p.as_str(), "1234-1242/tcp");
            assert_eq!(p.range().expect("range"), (1234, 1242));
        }

        // nat/nat_test.go: TestSplitProtoPort
        #[test]
        fn splitting_a_port_from_its_protocol_does_not_validate() {
            struct Case {
                doc: &'static str,
                input: &'static str,
                exp_port: &'static str,
                exp_proto: &'static str,
            }
            let cases = [
                Case {
                    doc: "empty value",
                    input: "",
                    exp_port: "",
                    exp_proto: "",
                },
                Case {
                    doc: "zero value",
                    input: "0",
                    exp_port: "0",
                    exp_proto: "tcp",
                },
                Case {
                    doc: "empty port",
                    input: "/udp",
                    exp_port: "",
                    exp_proto: "",
                },
                Case {
                    doc: "single port",
                    input: "1234",
                    exp_port: "1234",
                    exp_proto: "tcp",
                },
                Case {
                    doc: "single port with empty protocol",
                    input: "1234/",
                    exp_port: "1234",
                    exp_proto: "tcp",
                },
                Case {
                    doc: "single port with protocol",
                    input: "1234/udp",
                    exp_port: "1234",
                    exp_proto: "udp",
                },
                Case {
                    doc: "port range",
                    input: "80-8080",
                    exp_port: "80-8080",
                    exp_proto: "tcp",
                },
                Case {
                    doc: "port range with empty protocol",
                    input: "80-8080/",
                    exp_port: "80-8080",
                    exp_proto: "tcp",
                },
                Case {
                    doc: "port range with protocol",
                    input: "80-8080/udp",
                    exp_port: "80-8080",
                    exp_proto: "udp",
                },
                Case {
                    doc: "negative value",
                    input: "-1",
                    exp_port: "-1",
                    exp_proto: "tcp",
                },
                Case {
                    doc: "uppercase protocol",
                    input: "1234/UDP",
                    exp_port: "1234",
                    exp_proto: "UDP",
                },
                Case {
                    doc: "any value",
                    input: "any port value",
                    exp_port: "any port value",
                    exp_proto: "tcp",
                },
                Case {
                    doc: "any value with protocol",
                    input: "any port value/any proto value",
                    exp_port: "any port value",
                    exp_proto: "any proto value",
                },
            ];
            for tc in cases {
                let got = split_proto_port(tc.input);
                assert_eq!(got.proto, tc.exp_proto, "{}: proto", tc.doc);
                assert_eq!(got.port, tc.exp_port, "{}: port", tc.doc);
            }
        }

        // nat/nat_test.go: TestParsePortRange
        #[test]
        fn a_port_range_reports_strconv_errors_unwrapped_by_the_caller() {
            struct Case {
                doc: &'static str,
                input: &'static str,
                exp_begin: u16,
                exp_end: u16,
                exp_err: Option<&'static str>,
            }
            let cases = [
                Case {
                    doc: "empty value",
                    input: "",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: Some("empty string specified for ports"),
                },
                Case {
                    doc: "single port",
                    input: "1234",
                    exp_begin: 1234,
                    exp_end: 1234,
                    exp_err: None,
                },
                Case {
                    doc: "single port range",
                    input: "1234-1234",
                    exp_begin: 1234,
                    exp_end: 1234,
                    exp_err: None,
                },
                Case {
                    doc: "two port range",
                    input: "1234-1235",
                    exp_begin: 1234,
                    exp_end: 1235,
                    exp_err: None,
                },
                Case {
                    doc: "large range",
                    input: "8000-9000",
                    exp_begin: 8000,
                    exp_end: 9000,
                    exp_err: None,
                },
                Case {
                    doc: "zero port",
                    input: "0",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: None,
                },
                Case {
                    doc: "zero range",
                    input: "0-0",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: None,
                },
                // invalid cases
                Case {
                    doc: "non-numeric port",
                    input: "asdf",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: Some("strconv.ParseUint: parsing \"asdf\": invalid syntax"),
                },
                Case {
                    doc: "reversed range",
                    input: "9000-8000",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: Some("invalid range specified for port: 9000-8000"),
                },
                Case {
                    doc: "range missing end",
                    input: "8000-",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: Some("strconv.ParseUint: parsing \"\": invalid syntax"),
                },
                Case {
                    doc: "range missing start",
                    input: "-9000",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: Some("strconv.ParseUint: parsing \"\": invalid syntax"),
                },
                Case {
                    doc: "invalid range end",
                    input: "8000-a",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: Some("strconv.ParseUint: parsing \"a\": invalid syntax"),
                },
                Case {
                    doc: "invalid range end port",
                    input: "8000-9000a",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: Some("strconv.ParseUint: parsing \"9000a\": invalid syntax"),
                },
                Case {
                    doc: "range range start",
                    input: "a-9000",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: Some("strconv.ParseUint: parsing \"a\": invalid syntax"),
                },
                Case {
                    doc: "range range start port",
                    input: "8000a-9000",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: Some("strconv.ParseUint: parsing \"8000a\": invalid syntax"),
                },
                Case {
                    doc: "range with trailing hyphen",
                    input: "-8000-",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: Some("strconv.ParseUint: parsing \"\": invalid syntax"),
                },
                Case {
                    doc: "range without ports",
                    input: "-",
                    exp_begin: 0,
                    exp_end: 0,
                    exp_err: Some("strconv.ParseUint: parsing \"\": invalid syntax"),
                },
            ];
            for tc in cases {
                match parse_port_range(tc.input) {
                    Ok((begin, end)) => {
                        assert!(tc.exp_err.is_none(), "{}: unexpected ok", tc.doc);
                        assert_eq!(begin, tc.exp_begin, "{}: begin", tc.doc);
                        assert_eq!(end, tc.exp_end, "{}: end", tc.doc);
                    }
                    Err(err) => {
                        assert_eq!(Some(err.as_str()), tc.exp_err, "{}", tc.doc);
                        assert_eq!(0, tc.exp_begin, "{}: begin on error", tc.doc);
                        assert_eq!(0, tc.exp_end, "{}: end on error", tc.doc);
                    }
                }
            }
        }

        // nat/nat_test.go: TestParsePortSpecFull
        #[test]
        fn a_matched_pair_of_ranges_expands_port_by_port() {
            let mappings = parse_port_spec("0.0.0.0:1234-1235:3333-3334/tcp")
                .expect("a full spec with ranges");
            let expected = vec![
                PortMapping {
                    port: Port("3333/tcp".into()),
                    binding: PortBinding {
                        host_ip: "0.0.0.0".into(),
                        host_port: "1234".into(),
                    },
                },
                PortMapping {
                    port: Port("3334/tcp".into()),
                    binding: PortBinding {
                        host_ip: "0.0.0.0".into(),
                        host_port: "1235".into(),
                    },
                },
            ];
            assert_eq!(mappings, expected);
        }

        // nat/nat_test.go: TestPartPortSpecIPV6
        #[test]
        fn an_unbracketed_ipv6_host_is_re_joined_from_the_tail() {
            struct Case {
                name: &'static str,
                spec: &'static str,
                expected: Vec<PortMapping>,
            }
            let cases = [
                Case {
                    name: "square angled IPV6 without host port",
                    spec: "[2001:4860:0:2001::68]::333",
                    expected: vec![PortMapping {
                        port: Port("333/tcp".into()),
                        binding: PortBinding {
                            host_ip: "2001:4860:0:2001::68".into(),
                            host_port: String::new(),
                        },
                    }],
                },
                Case {
                    name: "square angled IPV6 with host port",
                    spec: "[::1]:80:80",
                    expected: vec![PortMapping {
                        port: Port("80/tcp".into()),
                        binding: PortBinding {
                            host_ip: "::1".into(),
                            host_port: "80".into(),
                        },
                    }],
                },
                Case {
                    name: "IPV6 without host port",
                    spec: "2001:4860:0:2001::68::333",
                    expected: vec![PortMapping {
                        port: Port("333/tcp".into()),
                        binding: PortBinding {
                            host_ip: "2001:4860:0:2001::68".into(),
                            host_port: String::new(),
                        },
                    }],
                },
                Case {
                    name: "IPV6 with host port",
                    spec: "::1:80:80",
                    expected: vec![PortMapping {
                        port: Port("80/tcp".into()),
                        binding: PortBinding {
                            host_ip: "::1".into(),
                            host_port: "80".into(),
                        },
                    }],
                },
                Case {
                    name: ":: IPV6, without host port",
                    spec: "::::80",
                    expected: vec![PortMapping {
                        port: Port("80/tcp".into()),
                        binding: PortBinding {
                            host_ip: "::".into(),
                            host_port: String::new(),
                        },
                    }],
                },
            ];
            for c in cases {
                assert_eq!(
                    parse_port_spec(c.spec).unwrap_or_else(|e| panic!("{}: {e}", c.name)),
                    c.expected,
                    "{}",
                    c.name
                );
            }
        }

        // nat/nat_test.go: TestParsePortSpecs, TestParsePortSpecsWithRange
        #[test]
        fn a_batch_of_specs_becomes_an_exposed_set_and_a_binding_map() {
            fn specs(input: &[&str]) -> Vec<String> {
                input.iter().map(|s| s.to_string()).collect()
            }
            // container port only
            let (ports, bindings) =
                parse_port_specs(&specs(&["1234/tcp", "2345/udp", "3456/sctp"]))
                    .expect("plain specs");
            for want in ["1234/tcp", "2345/udp", "3456/sctp"] {
                assert!(ports.contains(&Port(want.into())), "{want} was not exposed");
            }
            for (spec, bs) in &bindings {
                assert_eq!(bs.len(), 1, "{spec} should have exactly one binding");
                assert_eq!(bs[0].host_ip, "", "HostIP should not be set for {spec}");
                assert_eq!(bs[0].host_port, "", "HostPort should not be set for {spec}");
            }

            // host port equal to the container port, no address
            let (ports, bindings) = parse_port_specs(&specs(&[
                "1234:1234/tcp",
                "2345:2345/udp",
                "3456:3456/sctp",
            ]))
            .expect("host:container specs");
            for want in ["1234/tcp", "2345/udp", "3456/sctp"] {
                assert!(ports.contains(&Port(want.into())), "{want} was not exposed");
            }
            for (spec, bs) in &bindings {
                let port = split_proto_port(spec.as_str()).port;
                assert_eq!(bs.len(), 1, "{spec} should have exactly one binding");
                assert_eq!(bs[0].host_ip, "", "HostIP should not be set for {spec}");
                assert_eq!(bs[0].host_port, port, "HostPort for {spec}");
            }

            // an explicit address
            let (ports, bindings) = parse_port_specs(&specs(&[
                "0.0.0.0:1234:1234/tcp",
                "0.0.0.0:2345:2345/udp",
                "0.0.0.0:3456:3456/sctp",
            ]))
            .expect("address:host:container specs");
            for want in ["1234/tcp", "2345/udp", "3456/sctp"] {
                assert!(ports.contains(&Port(want.into())), "{want} was not exposed");
            }
            for (spec, bs) in &bindings {
                let port = split_proto_port(spec.as_str()).port;
                assert_eq!(bs.len(), 1, "{spec} should have exactly one binding");
                assert_eq!(bs[0].host_ip, "0.0.0.0", "HostIP for {spec}");
                assert_eq!(bs[0].host_port, port, "HostPort for {spec}");
            }

            // a hostname where an address belongs is the error to report
            assert!(parse_port_specs(&specs(&["localhost:1234:1234/tcp"])).is_err());
            assert!(parse_port_specs(&specs(&["localhost:1234-1236:1234-1236/tcp"])).is_err());
        }

        #[test]
        fn a_range_of_container_ports_is_exposed_individually() {
            fn specs(input: &[&str]) -> Vec<String> {
                input.iter().map(|s| s.to_string()).collect()
            }
            let (ports, bindings) = parse_port_specs(&specs(&[
                "1234-1236/tcp",
                "2345-2347/udp",
                "3456-3458/sctp",
            ]))
            .expect("container ranges");
            for want in ["1234/tcp", "1235/tcp", "1236/tcp", "2345/udp", "3456/sctp"] {
                assert!(ports.contains(&Port(want.into())), "{want} was not exposed");
            }
            for (spec, bs) in &bindings {
                assert_eq!(bs.len(), 1, "{spec} should have exactly one binding");
                assert_eq!(bs[0].host_ip, "", "HostIP should not be set for {spec}");
                assert_eq!(bs[0].host_port, "", "HostPort should not be set for {spec}");
            }

            let (_, bindings) = parse_port_specs(&specs(&[
                "1234-1236:1234-1236/tcp",
                "2345-2347:2345-2347/udp",
                "3456-3458:3456-3458/sctp",
            ]))
            .expect("matched ranges");
            for (spec, bs) in &bindings {
                let port = split_proto_port(spec.as_str()).port;
                assert_eq!(bs.len(), 1, "{spec} should have exactly one binding");
                assert_eq!(bs[0].host_ip, "", "HostIP should not be set for {spec}");
                assert_eq!(bs[0].host_port, port, "HostPort for {spec}");
            }

            let (_, bindings) = parse_port_specs(&specs(&[
                "0.0.0.0:1234-1236:1234-1236/tcp",
                "0.0.0.0:2345-2347:2345-2347/udp",
                "0.0.0.0:3456-3458:3456-3458/sctp",
            ]))
            .expect("matched ranges with an address");
            for (spec, bs) in &bindings {
                let port = split_proto_port(spec.as_str()).port;
                assert!(
                    bs.len() == 1 && bs[0].host_ip == "0.0.0.0" && bs[0].host_port == port,
                    "Expect single binding to port {port} but found {bs:?}"
                );
            }
        }

        // nat/nat_test.go: TestParseNetworkOptsPrivateOnly, Public, Udp, Sctp
        #[test]
        fn a_network_option_binds_an_address_and_maybe_a_port() {
            fn one(spec: &'static str) -> (String, String, String, String) {
                let (ports, bindings) =
                    parse_port_specs(&[spec.to_string()]).unwrap_or_else(|e| panic!("{spec}: {e}"));
                assert_eq!(
                    ports.len(),
                    1,
                    "{spec}: expected 1 exposed port, got {}",
                    ports.len()
                );
                assert_eq!(bindings.len(), 1, "{spec}: expected 1 binding entry");
                let (port, bs) = bindings.into_iter().next().expect("one entry");
                assert_eq!(bs.len(), 1, "{spec}: expected 1 binding");
                (
                    split_proto_port(port.as_str()).proto,
                    split_proto_port(port.as_str()).port,
                    bs[0].host_ip.clone(),
                    bs[0].host_port.clone(),
                )
            }

            assert_eq!(
                one("192.168.1.100::80"),
                ("tcp".into(), "80".into(), "192.168.1.100".into(), "".into())
            );
            assert_eq!(
                one("192.168.1.100:8080:80"),
                (
                    "tcp".into(),
                    "80".into(),
                    "192.168.1.100".into(),
                    "8080".into()
                )
            );
            assert_eq!(
                one("192.168.1.100::6000/udp"),
                (
                    "udp".into(),
                    "6000".into(),
                    "192.168.1.100".into(),
                    "".into()
                )
            );
            assert_eq!(
                one("192.168.1.100::6000/sctp"),
                (
                    "sctp".into(),
                    "6000".into(),
                    "192.168.1.100".into(),
                    "".into()
                )
            );
        }

        // nat/nat_test.go: TestParseNetworkOptsPublicNoPort, NegativePorts
        #[test]
        fn a_spec_that_is_only_an_address_or_has_a_negative_port_is_rejected() {
            // The address parses; the *container port* is the IP address.
            assert_eq!(
                parse_port_spec("192.168.1.100").expect_err("no port"),
                "invalid containerPort: 192.168.1.100"
            );
            assert_eq!(
                parse_port_spec("192.168.1.100:-1:-1").expect_err("negative port"),
                "invalid containerPort: -1"
            );
        }

        // nat/nat_test.go: TestStringer
        #[test]
        fn a_mapping_renders_as_a_host_port_pair() {
            let cases: &[(&str, &str, &str)] = &[
                ("no host mapping", ":8080:6000/tcp", ":8080:6000/tcp"),
                (
                    "no proto",
                    "192.168.1.100:8080:6000",
                    "192.168.1.100:8080:6000/tcp",
                ),
                (
                    "no host port",
                    "192.168.1.100::6000/udp",
                    "192.168.1.100::6000/udp",
                ),
                ("no mapping, port, or proto", "::6000", "::6000/tcp"),
                (
                    "ipv4 mapping",
                    "192.168.1.100:8080:6000/udp",
                    "192.168.1.100:8080:6000/udp",
                ),
                (
                    "ipv4 mapping without host port",
                    "192.168.1.100::6000/udp",
                    "192.168.1.100::6000/udp",
                ),
                ("ipv6 mapping", "[::1]:8080:6000/udp", "[::1]:8080:6000/udp"),
                (
                    "ipv6 mapping without host port",
                    "[::1]::6000/udp",
                    "[::1]::6000/udp",
                ),
                (
                    "ipv6 legacy mapping",
                    "::1:8080:6000/udp",
                    "[::1]:8080:6000/udp",
                ),
                (
                    "ipv6 legacy mapping without host port",
                    "::::6000/udp",
                    "[::]::6000/udp",
                ),
            ];
            for (doc, input, expected) in cases {
                let mappings = parse_port_spec(input).unwrap_or_else(|e| panic!("{doc}: {e}"));
                assert_eq!(mappings.len(), 1, "{doc}: all these produce one mapping");
                assert_eq!(mappings[0].to_string(), *expected, "{doc}");
            }
        }

        // Not upstream, but every one of these is a branch of ParsePortSpec
        // that no test reaches. Expected values read off go1.26.2.
        #[test]
        fn a_single_dynamic_host_range_is_allowed_next_to_one_port() {
            // The daemon allocates from the range, so it survives into HostPort.
            let mappings = parse_port_spec("8080-8090:80").expect("a dynamic host range");
            assert_eq!(mappings.len(), 1);
            assert_eq!(mappings[0].port.as_str(), "80/tcp");
            assert_eq!(mappings[0].binding.host_port, "8080-8090");
            // The other order is a mistake: a container range cannot be served
            // by an unrelated host port.
            assert_eq!(
                parse_port_spec("8080:80-90").expect_err("mismatched ranges"),
                "invalid ranges specified for container and host Ports: 80-90 and 8080"
            );
        }

        #[test]
        fn the_protocol_is_validated_before_anything_else_looks_at_the_spec() {
            // `80:` has an empty *container* port, and the `no port specified`
            // message would read as though it applied. It does not: the
            // protocol of the empty remainder is checked first.
            assert_eq!(parse_port_spec("").expect_err("empty"), "invalid proto: ");
            assert_eq!(
                parse_port_spec("80:").expect_err("no port"),
                "invalid proto: "
            );
            assert_eq!(
                parse_port_spec("80/xyz").expect_err("bad proto"),
                "invalid proto: xyz"
            );
            // A protocol is case-insensitive here, unlike in the moby parser.
            assert!(parse_port_spec("80/TCP").is_ok());
        }

        #[test]
        fn a_malformed_bracketed_address_is_reported_with_the_net_error() {
            // `splitParts` leaves `[::1` as the address once there is no closing
            // bracket, and `net.SplitHostPort` is what notices.
            assert_eq!(
                parse_port_spec("[::1:80:80").expect_err("unclosed bracket"),
                "invalid IP address [::1: address [::1:: missing ']' in address"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// github.com/moby/moby/api/types/network
// ---------------------------------------------------------------------------

/// The daemon-facing port types, which `parse()` uses to *re-validate* what
/// [`nat`] produced and to read `--expose`.
///
/// A second, stricter parser on purpose: `nat` keys its maps by string and
/// tolerates `"1234/UDP"`, while these refuse a range, normalise the protocol
/// to lower case and hold a validated `u16`. The zero value is a real state in
/// the Go API — `IsZero`, and `String` answering `"invalid port"` — so it is
/// modelled as a variant rather than as `Option` or a zero field.
pub mod network {
    use super::go_parse_uint;
    use std::fmt;
    use std::net::IpAddr;

    /// `network.IPProtocol`. Open, like the Go string type: `ncp` and
    /// `tcp:ipv6only` are legal, because nothing here validates it.
    pub const TCP: &str = "tcp";
    /// See [`TCP`].
    pub const UDP: &str = "udp";
    /// See [`TCP`].
    pub const SCTP: &str = "sctp";

    /// `network.Port`: a validated port number and protocol.
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Port {
        /// The zero value, which upstream says is invalid.
        Zero,
        /// A port built by [`parse_port`] or [`port_from`].
        Valid {
            /// The port number.
            num: u16,
            /// The normalised protocol.
            proto: String,
        },
    }

    impl Port {
        /// `p.Num()`, `0` for the zero value.
        pub fn num(&self) -> u16 {
            match self {
                Port::Zero => 0,
                Port::Valid { num, .. } => *num,
            }
        }

        /// `p.Proto()`, empty for the zero value.
        pub fn proto(&self) -> &str {
            match self {
                Port::Zero => "",
                Port::Valid { proto, .. } => proto,
            }
        }

        /// `p.IsZero()`.
        pub fn is_zero(&self) -> bool {
            matches!(self, Port::Zero)
        }

        /// `p.IsValid()` — the complement of [`Port::is_zero`], kept because
        /// upstream reads better at the call sites that use it.
        pub fn is_valid(&self) -> bool {
            !self.is_zero()
        }

        /// `p.Range()`: the port as a one-port range.
        pub fn range(&self) -> PortRange {
            match self {
                Port::Zero => PortRange::Zero,
                Port::Valid { num, proto } => PortRange::Valid {
                    start: *num,
                    end: *num,
                    proto: proto.clone(),
                },
            }
        }
    }

    impl fmt::Display for Port {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Port::Zero => f.write_str("invalid port"),
                Port::Valid { num, proto } => write!(f, "{num}/{proto}"),
            }
        }
    }

    /// `network.PortRange`: a validated port range and protocol.
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum PortRange {
        /// The zero value, which upstream says is invalid.
        Zero,
        /// A range built by [`parse_port_range`] or [`port_range_from`].
        Valid {
            /// The first port.
            start: u16,
            /// The last port, never below `start`.
            end: u16,
            /// The normalised protocol.
            proto: String,
        },
    }

    impl PortRange {
        /// `pr.Start()`, `0` for the zero value.
        pub fn start(&self) -> u16 {
            match self {
                PortRange::Zero => 0,
                PortRange::Valid { start, .. } => *start,
            }
        }

        /// `pr.End()`, `0` for the zero value.
        pub fn end(&self) -> u16 {
            match self {
                PortRange::Zero => 0,
                PortRange::Valid { end, .. } => *end,
            }
        }

        /// `pr.Proto()`, empty for the zero value.
        pub fn proto(&self) -> &str {
            match self {
                PortRange::Zero => "",
                PortRange::Valid { proto, .. } => proto,
            }
        }

        /// `pr.IsZero()`.
        pub fn is_zero(&self) -> bool {
            matches!(self, PortRange::Zero)
        }

        /// `pr.IsValid()`.
        pub fn is_valid(&self) -> bool {
            !self.is_zero()
        }

        /// `pr.Range()` — a range is its own range.
        pub fn range(&self) -> PortRange {
            self.clone()
        }

        /// `pr.All()`: every port in the range, lazily, so a consumer can stop
        /// early — the `1000-2000/tcp` case takes the first two and walks away.
        ///
        /// The iteration is over `u32` because a full `0-65535` range has
        /// 65 536 entries and a `u16` counter would wrap.
        pub fn all(&self) -> impl Iterator<Item = Port> + '_ {
            let (start, end, proto) = match self {
                PortRange::Zero => (0, 0, String::new()),
                PortRange::Valid { start, end, proto } => (*start, *end, proto.clone()),
            };
            (start as u32..=end as u32).map(move |i| Port::Valid {
                num: i as u16,
                proto: proto.clone(),
            })
        }
    }

    impl fmt::Display for PortRange {
        /// A one-port range renders as a bare port, not as `1234-1234/tcp`.
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                PortRange::Zero => f.write_str("invalid port range"),
                PortRange::Valid { start, end, proto } if start == end => {
                    write!(f, "{start}/{proto}")
                }
                PortRange::Valid { start, end, proto } => write!(f, "{start}-{end}/{proto}"),
            }
        }
    }

    /// `network.PortBinding`: the host side of a published port.
    ///
    /// `netip.Addr`'s invalid zero value is `None` here. A `Some` is always a
    /// real, parsed address, which Go's type also guarantees.
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct PortBinding {
        /// The host address, or every address when `None`.
        pub host_ip: Option<IpAddr>,
        /// The host port or range, empty for a random one.
        pub host_port: String,
    }

    /// `normalizePortProto`: lower case, defaulting to `tcp`.
    fn normalize_port_proto(proto: &str) -> String {
        if proto.is_empty() {
            TCP.to_string()
        } else {
            proto.to_lowercase()
        }
    }

    /// `parsePortNumber`: the reason only, without `strconv`'s wrapper.
    ///
    /// Unlike [`super::nat::parse_port`] this reports the *empty* case
    /// separately from a syntax error, which is what lets `--expose=/tcp` say
    /// `value is empty` where `--expose=NaN` says `invalid syntax`.
    fn parse_port_number(raw_port: &str) -> Result<u16, &'static str> {
        if raw_port.is_empty() {
            return Err("value is empty");
        }
        go_parse_uint(raw_port, 16).map(|p| p as u16)
    }

    /// `network.ParsePort`: a single validated port.
    ///
    /// A range is *not* accepted here, so `1234-1240` is a syntax error; that
    /// is the difference from [`parse_port_range`] that `parse()` relies on
    /// when it runs this over `nat`'s output.
    pub fn parse_port(s: &str) -> Result<Port, String> {
        if s.is_empty() {
            return Err("invalid port: value is empty".to_string());
        }
        let (port, proto) = match s.split_once('/') {
            Some((port, proto)) => (port, proto),
            None => (s, ""),
        };
        let num =
            parse_port_number(port).map_err(|reason| format!("invalid port '{port}': {reason}"))?;
        Ok(Port::Valid {
            num,
            proto: normalize_port_proto(proto),
        })
    }

    /// `network.ParsePortRange`: a validated range, or a single port.
    ///
    /// `start == end` is accepted and collapses, so `1234-1234` and `1234` are
    /// the same range. The *start* is validated before the protocol is
    /// normalised and the end is validated last, which is why `--expose=8080-
    /// NaN` complains about the end and `--expose=NaN-NaN` about the start.
    pub fn parse_port_range(s: &str) -> Result<PortRange, String> {
        if s.is_empty() {
            return Err("invalid port range: value is empty".to_string());
        }
        let (port_range, proto) = match s.split_once('/') {
            Some((port_range, proto)) => (port_range, proto),
            None => (s, ""),
        };
        let (start, end) = match port_range.split_once('-') {
            Some((start, end)) => (start, end),
            None => (port_range, port_range),
        };
        let start_val = parse_port_number(start)
            .map_err(|reason| format!("invalid start port '{start}': {reason}"))?;
        let port_proto = normalize_port_proto(proto);
        if start == end {
            return Ok(PortRange::Valid {
                start: start_val,
                end: start_val,
                proto: port_proto,
            });
        }
        let end_val = parse_port_number(end)
            .map_err(|reason| format!("invalid end port '{end}': {reason}"))?;
        if end_val < start_val {
            return Err(format!("invalid port range: {s}"));
        }
        Ok(PortRange::Valid {
            start: start_val,
            end: end_val,
            proto: port_proto,
        })
    }

    /// `network.PortFrom`. An empty protocol yields the zero value, not `tcp`:
    /// this constructor requires a protocol, unlike the parsers.
    pub fn port_from(num: u16, proto: &str) -> (Port, bool) {
        if proto.is_empty() {
            return (Port::Zero, false);
        }
        (
            Port::Valid {
                num,
                proto: normalize_port_proto(proto),
            },
            true,
        )
    }

    /// `network.PortRangeFrom`. Rejects an inverted range and an empty
    /// protocol.
    pub fn port_range_from(start: u16, end: u16, proto: &str) -> (PortRange, bool) {
        if end < start || proto.is_empty() {
            return (PortRange::Zero, false);
        }
        (
            PortRange::Valid {
                start,
                end,
                proto: normalize_port_proto(proto),
            },
            true,
        )
    }

    #[cfg(test)]
    mod tests {
        use super::{super::ERR_RANGE, super::ERR_SYNTAX, *};

        fn port(num: u16, proto: &str) -> Port {
            port_from(num, proto).0
        }

        fn range(start: u16, end: u16, proto: &str) -> PortRange {
            port_range_from(start, end, proto).0
        }

        // port_test.go: TestPort/Zero Value, TestPortRange/Zero Value
        #[test]
        fn the_zero_value_is_invalid_and_says_so() {
            let p = Port::Zero;
            assert!(p.is_zero());
            assert!(!p.is_valid());
            assert_eq!(p.to_string(), "invalid port");
            assert_eq!(p.num(), 0);
            assert_eq!(p.proto(), "");
            assert!(p.range().is_zero());

            let pr = PortRange::Zero;
            assert!(pr.is_zero());
            assert!(!pr.is_valid());
            assert_eq!(pr.to_string(), "invalid port range");
            assert_eq!(pr.start(), 0);
            assert_eq!(pr.end(), 0);
            assert_eq!(pr.proto(), "");
        }

        // port_test.go: TestPort/PortFrom
        #[test]
        fn a_port_needs_a_protocol_to_be_built() {
            for (num, proto) in [
                (0, TCP),
                (80, TCP),
                (8080, TCP),
                (65535, TCP),
                (80, UDP),
                (8080, SCTP),
            ] {
                let (p, ok) = port_from(num, proto);
                assert!(ok, "{num}_{proto}");
                assert_eq!(p.num(), num);
                assert_eq!(p.proto(), proto);
            }
            // The protocol is normalised, not validated.
            assert_eq!(port(1234, "tcp"), port(1234, "TCP"));
            assert_eq!(port(1234, "TCP"), port(1234, "tCp"));

            for (num, proto) in [(0, ""), (80, "")] {
                let (p, ok) = port_from(num, proto);
                assert!(!ok, "{num}_{proto}");
                assert!(p.is_zero());
                assert!(!p.is_valid());
                assert_eq!(p.to_string(), "invalid port");
            }
        }

        // port_test.go: TestPort/ParsePort
        #[test]
        fn a_valid_port_parses_and_round_trips() {
            struct Case {
                input: &'static str,
                port: Port,
                str: &'static str,
            }
            let cases = [
                // Zero port
                Case {
                    input: "0/tcp",
                    port: port(0, TCP),
                    str: "0/tcp",
                },
                // Max valid port
                Case {
                    input: "65535/tcp",
                    port: port(65535, TCP),
                    str: "65535/tcp",
                },
                // Simple valid ports
                Case {
                    input: "1234/tcp",
                    port: port(1234, TCP),
                    str: "1234/tcp",
                },
                Case {
                    input: "1234/udp",
                    port: port(1234, UDP),
                    str: "1234/udp",
                },
                Case {
                    input: "1234/sctp",
                    port: port(1234, SCTP),
                    str: "1234/sctp",
                },
                // Default protocol is tcp
                Case {
                    input: "1234",
                    port: port(1234, TCP),
                    str: "1234/tcp",
                },
                // Default protocol is tcp
                Case {
                    input: "1234/",
                    port: port(1234, TCP),
                    str: "1234/tcp",
                },
                // An unvalidated protocol keeps everything after the slash.
                Case {
                    input: "1234/tcp:ipv6only",
                    port: port(1234, "tcp:ipv6only"),
                    str: "1234/tcp:ipv6only",
                },
            ];
            for tc in cases {
                let got = parse_port(tc.input).unwrap_or_else(|e| panic!("{}: {e}", tc.input));
                assert_eq!(got, tc.port, "{}", tc.input);
                assert!(!got.is_zero());
                assert!(got.is_valid());
                // Purity.
                assert_eq!(parse_port(tc.input).expect("again"), got, "{}", tc.input);
                // Identity: the canonical form parses back to the same port.
                assert_eq!(
                    parse_port(&got.to_string()).expect("identity"),
                    got,
                    "{}",
                    tc.input
                );
                assert_eq!(got.to_string(), tc.str, "{}", tc.input);
                // A port is the one-port range of itself.
                assert_eq!(got.range(), range(got.num(), got.num(), got.proto()));
            }
        }

        // port_test.go: TestPort/ParsePort negative tests
        #[test]
        fn a_rejected_port_says_why() {
            let cases: &[(&str, &str)] = &[
                // Empty string
                ("", "invalid port: value is empty"),
                // Whitespace-only string
                (" ", "invalid port ' ': invalid syntax"),
                // No port number
                ("/", "invalid port '': value is empty"),
                // No port number (protocol only)
                ("/tcp", "invalid port '': value is empty"),
                // Negative port
                ("-1", "invalid port '-1': invalid syntax"),
                // Too large port
                ("65536", "invalid port '65536': value out of range"),
                // Non-numeric port
                ("foo", "invalid port 'foo': invalid syntax"),
                // Port range instead of single port
                ("1234-1240/udp", "invalid port '1234-1240': invalid syntax"),
                // Port range instead of single port without protocol
                ("1234-1240", "invalid port '1234-1240': invalid syntax"),
                // Garbage port
                ("asd1234/tcp", "invalid port 'asd1234': invalid syntax"),
            ];
            for (input, expected) in cases {
                let err = parse_port(input).expect_err(input);
                assert!(err.contains("invalid port"), "{input}: {err}");
                assert_eq!(&err, expected, "{input}");
            }
        }

        // port_test.go: TestPortRange/PortRangeFrom
        #[test]
        fn a_port_range_needs_a_protocol_and_an_ordered_range() {
            for (start, end, proto) in [
                (0, 0, TCP),
                (0, 1234, TCP),
                (80, 80, TCP),
                (80, 8080, TCP),
                (1234, 65535, TCP),
                (80, 80, UDP),
                (80, 8080, SCTP),
            ] {
                let (pr, ok) = port_range_from(start, end, proto);
                assert!(ok, "{start}_{end}_{proto}");
                assert_eq!(pr.start(), start);
                assert_eq!(pr.end(), end);
                assert_eq!(pr.proto(), proto);
            }
            assert_eq!(range(1234, 5678, "tcp"), range(1234, 5678, "TCP"));
            assert_eq!(range(1234, 5678, "TCP"), range(1234, 5678, "tCp"));

            for (start, end, proto) in [
                (1234, 80, TCP), // end < start
                (0, 0, ""),      // empty protocol
            ] {
                let (pr, ok) = port_range_from(start, end, proto);
                assert!(!ok, "{start}_{end}_{proto}");
                assert!(pr.is_zero());
                assert!(!pr.is_valid());
            }
        }

        // port_test.go: TestPortRange/ParsePortRange
        #[test]
        fn a_valid_port_range_parses_and_round_trips() {
            struct Case {
                input: &'static str,
                port_range: PortRange,
                str: &'static str,
            }
            let cases = [
                // Zero port
                Case {
                    input: "0-1234/tcp",
                    port_range: range(0, 1234, TCP),
                    str: "0-1234/tcp",
                },
                // Max valid port
                Case {
                    input: "1234-65535/tcp",
                    port_range: range(1234, 65535, TCP),
                    str: "1234-65535/tcp",
                },
                // Simple valid ports
                Case {
                    input: "1234-4567/tcp",
                    port_range: range(1234, 4567, TCP),
                    str: "1234-4567/tcp",
                },
                Case {
                    input: "1234-4567/udp",
                    port_range: range(1234, 4567, UDP),
                    str: "1234-4567/udp",
                },
                // Default protocol is tcp
                Case {
                    input: "1234-4567",
                    port_range: range(1234, 4567, TCP),
                    str: "1234-4567/tcp",
                },
                // Default protocol is tcp
                Case {
                    input: "1234-4567/",
                    port_range: range(1234, 4567, TCP),
                    str: "1234-4567/tcp",
                },
                // A one-port range collapses to a port.
                Case {
                    input: "1234/tcp",
                    port_range: range(1234, 1234, TCP),
                    str: "1234/tcp",
                },
                Case {
                    input: "1234",
                    port_range: range(1234, 1234, TCP),
                    str: "1234/tcp",
                },
                Case {
                    input: "1234-5678/tcp:ipv6only",
                    port_range: range(1234, 5678, "tcp:ipv6only"),
                    str: "1234-5678/tcp:ipv6only",
                },
            ];
            for tc in cases {
                let got =
                    parse_port_range(tc.input).unwrap_or_else(|e| panic!("{}: {e}", tc.input));
                assert_eq!(got, tc.port_range, "{}", tc.input);
                assert!(!got.is_zero());
                assert!(got.is_valid());
                assert_eq!(
                    parse_port_range(tc.input).expect("again"),
                    got,
                    "{}",
                    tc.input
                );
                assert_eq!(
                    parse_port_range(&got.to_string()).expect("identity"),
                    got,
                    "{}",
                    tc.input
                );
                assert_eq!(got.to_string(), tc.str, "{}", tc.input);
                assert_eq!(got.range(), tc.port_range, "{}", tc.input);
            }
        }

        // port_test.go: TestPortRange/ParsePortRange negative tests
        #[test]
        fn a_rejected_port_range_says_which_end_is_wrong() {
            let cases: &[(&str, &str)] = &[
                // Empty string
                ("", "invalid port range: value is empty"),
                // Whitespace-only string
                (" ", "invalid start port ' ': invalid syntax"),
                // No port number
                ("/", "invalid start port '': value is empty"),
                // No port number (protocol only)
                ("/tcp", "invalid start port '': value is empty"),
                // Negative start port: the leading '-' is the separator, so
                // what is left to complain about is the empty start.
                ("-1-1234", "invalid start port '': value is empty"),
                // Negative end port
                ("1234--1", "invalid end port '-1': invalid syntax"),
                // Too large start port
                (
                    "65536-65537",
                    "invalid start port '65536': value out of range",
                ),
                // Too large end port
                ("1234-65536", "invalid end port '65536': value out of range"),
                // Non-numeric start port
                ("foo-1234", "invalid start port 'foo': invalid syntax"),
                // Non-numeric end port
                ("1234-bar", "invalid end port 'bar': invalid syntax"),
                // Start port greater than end port
                ("1234-1000", "invalid port range: 1234-1000"),
                // Garbage port range
                (
                    "asd1234-5678/tcp",
                    "invalid start port 'asd1234': invalid syntax",
                ),
            ];
            for (input, expected) in cases {
                let err = parse_port_range(input).expect_err(input);
                assert_eq!(&err, expected, "{input}");
            }
        }

        // port_test.go: TestPortRange/PortRange All()
        #[test]
        fn all_walks_every_port_in_the_range() {
            let cases: &[(&str, Vec<Port>)] = &[
                ("1000-1000/tcp", vec![port(1000, TCP)]),
                (
                    "1000-1002/tcp",
                    vec![port(1000, TCP), port(1001, TCP), port(1002, TCP)],
                ),
                ("0-0/tcp", vec![port(0, TCP)]),
                ("65535-65535/tcp", vec![port(65535, TCP)]),
                (
                    "65530-65535/tcp",
                    (65530..=65535).map(|n| port(n, TCP)).collect(),
                ),
            ];
            for (input, want) in cases {
                let pr = parse_port_range(input).expect(input);
                assert_eq!(pr.all().collect::<Vec<_>>(), *want, "{input}");
            }

            // All() stop early
            let want = vec![port(1000, TCP), port(1001, TCP)];
            let pr = parse_port_range("1000-2000/tcp").expect("a range");
            let got: Vec<Port> = pr.all().take(2).collect();
            assert_eq!(got, want);
        }

        // Not upstream: the two `strconv` reasons are the whole contract of
        // these two functions, and neither is otherwise asserted in isolation.
        #[test]
        fn the_bare_strconv_reasons_are_the_ones_go_reports() {
            assert_eq!(super::super::go_parse_uint("1234", 16), Ok(1234));
            assert_eq!(super::super::go_parse_uint("0123", 16), Ok(123));
            assert_eq!(super::super::go_parse_uint("+1", 16), Err(ERR_SYNTAX));
            assert_eq!(super::super::go_parse_uint("0x10", 16), Err(ERR_SYNTAX));
            assert_eq!(super::super::go_parse_uint("1_2", 16), Err(ERR_SYNTAX));
            assert_eq!(super::super::go_parse_uint("", 16), Err(ERR_SYNTAX));
            assert_eq!(super::super::go_parse_uint("65536", 16), Err(ERR_RANGE));
            assert_eq!(
                super::super::go_parse_uint("999999999999999999999999", 16),
                Err(ERR_RANGE)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// docker/cli internal/volumespec, reached through loader.ParseVolume
// ---------------------------------------------------------------------------

/// The `-v` grammar.
///
/// `loader.ParseVolume` has been a one-line shim over `internal/volumespec`
/// since v28; this is the code behind it, and it is deliberately
/// platform-independent — the Windows handling is *about* the spec's text, not
/// about the host it runs on.
pub mod volume {
    /// `mount.TypeBind`: a host path.
    pub const TYPE_BIND: &str = "bind";
    /// `mount.TypeVolume`: a named volume, or an anonymous one.
    pub const TYPE_VOLUME: &str = "volume";
    /// `mount.TypeTmpfs`.
    pub const TYPE_TMPFS: &str = "tmpfs";
    /// `mount.TypeNamedPipe`.
    pub const TYPE_NAMED_PIPE: &str = "npipe";
    /// `mount.TypeCluster`.
    pub const TYPE_CLUSTER: &str = "cluster";
    /// `mount.TypeImage`.
    pub const TYPE_IMAGE: &str = "image";

    /// `mount.Propagations`, the option values that name a bind propagation.
    const PROPAGATIONS: &[&str] = &[
        "rprivate", "private", "rshared", "shared", "rslave", "slave",
    ];

    /// The end-of-spec sentinel `volumespec` appends to the spec.
    ///
    /// A rune, not a byte, because `Parse` iterates *runes* — which is what
    /// makes the drive-letter check below work on a multi-byte path and stops
    /// `a界` from being mistaken for one.
    const END_OF_SPEC: char = '\0';

    /// The option families `volumespec.Parse` can produce.
    ///
    /// Go carries these as five independent pointer fields on `VolumeConfig`,
    /// any two of which could in principle be set at once; `Parse` only ever
    /// sets one. An enum keeps the states Go cannot reach unreachable.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum MountOpts {
        /// A bind propagation, from `:rprivate` and friends.
        Bind {
            /// The propagation, absent when the option was not one.
            propagation: Option<String>,
        },
        /// `:nocopy`.
        Volume {
            /// Whether the volume's initial contents are skipped.
            no_copy: bool,
        },
    }

    /// `volumespec.VolumeConfig`, minus the fields `Parse` never fills.
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct VolumeConfig {
        /// One of the `TYPE_*` constants. Empty when `Parse` failed before it
        /// could classify the spec.
        pub mount_type: String,
        /// The host path or volume name. Empty for an anonymous volume, which
        /// is how `parse()` tells a bind from a plain volume.
        pub source: String,
        /// The path inside the container.
        pub target: String,
        /// The `:ro` flag.
        pub read_only: bool,
        /// The parsed option field, if the spec had one.
        pub opts: Option<MountOpts>,
    }

    /// `isWindowsDrive`: a colon after exactly one letter is a drive letter,
    /// not a field separator.
    ///
    /// This is the whole of the Windows support `-v` needs, and it is why
    /// `C:\path` is one field and not `C` + `\path`. The letter test is
    /// Unicode-aware upstream, so `é:x` is a drive too.
    fn is_windows_drive(buffer: &[char], char: char) -> bool {
        char == ':' && buffer.len() == 1 && buffer[0].is_alphabetic()
    }

    /// `isFilePath`: does this source name a host path rather than a volume?
    ///
    /// A leading `.`, `/` or `~`, a UNC named pipe, or a drive letter. A bare
    /// single character is *not* a path — that is how `c:` alone stays a target.
    fn is_file_path(source: &str) -> bool {
        let Some(first_byte) = source.as_bytes().first() else {
            // `populateType` only calls this with a non-empty source; Go would
            // panic on `source[0]`.
            return false;
        };
        if matches!(first_byte, b'.' | b'/' | b'~') {
            return true;
        }
        if source.chars().count() == 1 {
            return false;
        }
        // Windows named pipes
        if source.starts_with(r"\\") {
            return true;
        }
        // Upstream reads the *byte* after the first rune, not the next rune: for
        // `a界` that is 0xE7, which is of course not a colon. Taking the next
        // `char` would silently change the answer for any multi-byte path.
        let first = source.chars().next().expect("checked non-empty");
        let next_index = first.len_utf8();
        let byte_after = source.as_bytes()[next_index] as char;
        is_windows_drive(&[first], byte_after)
    }

    /// `populateFieldFromBuffer`: assign one colon-separated field, or read the
    /// option field.
    ///
    /// The `switch` is a fall-through ladder, and the order is the behaviour:
    /// a lone field at end-of-spec is the *target* of an anonymous volume, the
    /// first two fields are source and target in that order, and only a *fourth*
    /// field can be an option list. Everything after that is ignored.
    fn populate_field_from_buffer(
        char: char,
        buffer: &[char],
        volume: &mut VolumeConfig,
    ) -> Result<(), String> {
        let str_buffer: String = buffer.iter().collect();
        if buffer.is_empty() {
            return Err("empty section between colons".to_string());
        }
        if volume.source.is_empty() {
            if char == END_OF_SPEC {
                // Anonymous volume
                volume.target = str_buffer;
            } else {
                volume.source = str_buffer;
            }
            return Ok(());
        }
        if volume.target.is_empty() {
            volume.target = str_buffer;
            return Ok(());
        }
        if char == ':' {
            return Err("too many colons".to_string());
        }
        for option in str_buffer.split(',') {
            match option {
                "ro" => volume.read_only = true,
                "rw" => volume.read_only = false,
                "nocopy" => {
                    volume.opts = Some(MountOpts::Volume { no_copy: true });
                }
                other => {
                    if PROPAGATIONS.contains(&other) {
                        volume.opts = Some(MountOpts::Bind {
                            propagation: Some(other.to_string()),
                        });
                    }
                    // Unknown options are ignored: `Z`, `z` and `cached` are
                    // real SELinux and consistency options that belong to a
                    // later layer, and act's binds drop them anyway.
                }
            }
        }
        Ok(())
    }

    /// `populateType`: anonymous volume, host path, or named volume.
    fn populate_type(volume: &mut VolumeConfig) {
        volume.mount_type = if volume.source.is_empty() {
            TYPE_VOLUME.to_string()
        } else if is_file_path(&volume.source) {
            TYPE_BIND.to_string()
        } else {
            TYPE_VOLUME.to_string()
        };
    }

    /// `loader.ParseVolume`: one `-v` spec, as a bind or a volume.
    ///
    /// # The two return values are not one `Result`
    ///
    /// Go returns a *partly filled* config alongside the error, because the
    /// error path calls `populateType` before bailing out — so `/foo::ro` fails
    /// and still reports itself a bind. `TestParseVolumeSplitCases` asserts on
    /// that, so folding the config into an error type would be a silent
    /// simplification; the config stays separate, as in Go. [`parse_volume`]
    /// wraps the pair for the callers that only want to fail loudly.
    ///
    /// # A spec of one or two characters skips the parser entirely
    ///
    /// `len(spec)` is counted in **bytes** and checked before any splitting,
    /// which is why `.` and `d:` are both a bare target and never reach the
    /// colon logic.
    pub fn parse(spec: &str) -> (VolumeConfig, Result<(), String>) {
        let mut volume = VolumeConfig::default();
        match spec.len() {
            0 => return (volume, Err("invalid empty volume spec".to_string())),
            1 | 2 => {
                volume.target = spec.to_string();
                volume.mount_type = TYPE_VOLUME.to_string();
                return (volume, Ok(()));
            }
            _ => {}
        }

        let mut buffer: Vec<char> = Vec::with_capacity(spec.len());
        for char in spec.chars().chain(std::iter::once(END_OF_SPEC)) {
            if is_windows_drive(&buffer, char) {
                buffer.push(char);
            } else if char == ':' || char == END_OF_SPEC {
                if let Err(err) = populate_field_from_buffer(char, &buffer, &mut volume) {
                    populate_type(&mut volume);
                    return (volume, Err(format!("invalid spec: {spec}: {err}")));
                }
                buffer.clear();
            } else {
                buffer.push(char);
            }
        }

        populate_type(&mut volume);
        (volume, Ok(()))
    }

    /// [`parse`], discarding the partly filled config on failure.
    ///
    /// This is the shape act's `parse()` wants: it stops at the first bad
    /// `-v` and never looks at the config.
    pub fn parse_volume(spec: &str) -> Result<VolumeConfig, String> {
        let (volume, result) = parse(spec);
        result.map(|()| volume)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn expect(spec: &str) -> VolumeConfig {
            let (volume, err) = parse(spec);
            err.unwrap_or_else(|e| panic!("{spec:?}: {e}"));
            volume
        }

        fn expect_err(spec: &str) -> String {
            let (_, err) = parse(spec);
            err.err()
                .unwrap_or_else(|| panic!("{spec:?} should not parse"))
        }

        // volumespec_test.go: TestParseVolumeAnonymousVolume,
        // TestParseVolumeAnonymousVolumeWindows
        #[test]
        fn a_bare_path_is_an_anonymous_volume() {
            for path in ["/path", "/path/foo", "C:\\path", "Z:\\path\\foo"] {
                let volume = expect(path);
                assert_eq!(
                    volume,
                    VolumeConfig {
                        mount_type: TYPE_VOLUME.into(),
                        target: path.into(),
                        ..VolumeConfig::default()
                    },
                    "{path}"
                );
            }
        }

        // volumespec_test.go: TestParseVolumeTooManyColons
        #[test]
        fn a_fourth_field_is_too_many_colons() {
            assert_eq!(
                expect_err("/foo:/foo:ro:foo"),
                "invalid spec: /foo:/foo:ro:foo: too many colons"
            );
        }

        // volumespec_test.go: TestParseVolumeShortVolumes
        #[test]
        fn a_one_or_two_character_spec_is_never_split() {
            for path in [".", "/a"] {
                let volume = expect(path);
                assert_eq!(
                    volume,
                    VolumeConfig {
                        mount_type: TYPE_VOLUME.into(),
                        target: path.into(),
                        ..VolumeConfig::default()
                    },
                    "{path}"
                );
            }
        }

        // volumespec_test.go: TestParseVolumeMissingSource,
        // TestParseVolumeWithEmptySource, TestParseVolumeInvalidSections,
        // TestParseVolumeInvalidEmptySpec
        #[test]
        fn an_empty_field_between_colons_is_an_error() {
            for spec in [":foo", "/foo::ro", ":/vol"] {
                assert!(
                    expect_err(spec).contains("empty section between colons"),
                    "{spec}"
                );
            }
            assert!(expect_err("/foo::rw").contains("invalid spec"));
            assert_eq!(expect_err(""), "invalid empty volume spec");
        }

        // volumespec_test.go: TestParseVolumeBindMount
        #[test]
        fn a_path_with_a_colon_is_a_bind() {
            for path in ["./foo", "~/thing", "../other", "/foo", "/home/user"] {
                let volume = expect(&format!("{path}:/target"));
                assert_eq!(
                    volume,
                    VolumeConfig {
                        mount_type: TYPE_BIND.into(),
                        source: path.into(),
                        target: "/target".into(),
                        ..VolumeConfig::default()
                    },
                    "{path}"
                );
            }
        }

        // volumespec_test.go: TestParseVolumeRelativeBindMountWindows
        #[test]
        fn a_relative_path_binds_to_a_drive_lettered_target() {
            for path in ["./foo", "~/thing", "../other", "D:\\path", "/home/user"] {
                let volume = expect(&format!("{path}:d:\\target"));
                assert_eq!(
                    volume,
                    VolumeConfig {
                        mount_type: TYPE_BIND.into(),
                        source: path.into(),
                        target: "d:\\target".into(),
                        ..VolumeConfig::default()
                    },
                    "{path}"
                );
            }
        }

        // volumespec_test.go: TestParseVolumeWithBindOptions,
        // TestParseVolumeWithBindOptionsWindows
        #[test]
        fn a_propagation_option_is_kept_and_a_windows_source_keeps_its_drive() {
            assert_eq!(
                expect("/source:/target:slave"),
                VolumeConfig {
                    mount_type: TYPE_BIND.into(),
                    source: "/source".into(),
                    target: "/target".into(),
                    opts: Some(MountOpts::Bind {
                        propagation: Some("slave".into())
                    }),
                    ..VolumeConfig::default()
                }
            );
            assert_eq!(
                expect("C:\\source\\foo:D:\\target:ro,rprivate"),
                VolumeConfig {
                    mount_type: TYPE_BIND.into(),
                    source: "C:\\source\\foo".into(),
                    target: "D:\\target".into(),
                    read_only: true,
                    opts: Some(MountOpts::Bind {
                        propagation: Some("rprivate".into())
                    }),
                }
            );
        }

        // volumespec_test.go: TestParseVolumeWithInvalidVolumeOptions
        #[test]
        fn an_unknown_option_is_not_an_error() {
            expect("name:/target:bogus");
        }

        // volumespec_test.go: TestParseVolumeWithVolumeOptions
        #[test]
        fn nocopy_marks_the_volume_and_leaves_it_a_volume() {
            assert_eq!(
                expect("name:/target:nocopy"),
                VolumeConfig {
                    mount_type: TYPE_VOLUME.into(),
                    source: "name".into(),
                    target: "/target".into(),
                    opts: Some(MountOpts::Volume { no_copy: true }),
                    ..VolumeConfig::default()
                }
            );
        }

        // volumespec_test.go: TestParseVolumeWithReadOnly, TestParseVolumeWithRW
        #[test]
        fn ro_and_rw_set_the_read_only_flag() {
            for (mode, read_only) in [("ro", true), ("rw", false)] {
                for path in ["./foo", "/home/user"] {
                    let volume = expect(&format!("{path}:/target:{mode}"));
                    assert_eq!(
                        volume,
                        VolumeConfig {
                            mount_type: TYPE_BIND.into(),
                            source: path.into(),
                            target: "/target".into(),
                            read_only,
                            ..VolumeConfig::default()
                        },
                        "{path}:{mode}"
                    );
                }
            }
        }

        // volumespec_test.go: TestParseVolumeWindowsNamedPipe
        #[test]
        fn a_unc_named_pipe_is_a_bind() {
            assert_eq!(
                expect(r"\\.\pipe\docker_engine:\\.\pipe\inside"),
                VolumeConfig {
                    mount_type: TYPE_BIND.into(),
                    source: r"\\.\pipe\docker_engine".into(),
                    target: r"\\.\pipe\inside".into(),
                    ..VolumeConfig::default()
                }
            );
        }

        // volumespec_test.go: TestIsFilePath
        #[test]
        fn a_bare_word_is_not_a_host_path() {
            assert!(!is_file_path("a界"));
            assert!(!is_file_path("1"));
            assert!(!is_file_path("c"));
        }

        // volumespec_test.go: TestParseVolumeSplitCases
        #[test]
        fn the_split_table_still_agrees_about_which_specs_have_a_source() {
            // The upstream table also records the expected field list, which
            // `Parse` no longer returns; what it asserts is that a spec with
            // more than one field has a source and one without does not.
            let cases: &[(&str, Option<&[&str]>)] = &[
                (r"C:\foo:d:", Some(&[r"C:\foo", "d:"])),
                (r":C:\foo:d:", None),
                (r"C:\foo\:/foo", Some(&[r"C:\foo\", "/foo"])),
                (r"d:\", Some(&[r"d:\"])),
                (r"d:\pathandmode:rw", Some(&[r"d:\pathandmode", "rw"])),
                (r"c:\:d:\", Some(&[r"c:\", r"d:\"])),
                (
                    r"c:\windows:d:\s p a c e:RW",
                    Some(&[r"c:\windows", r"d:\s p a c e", "RW"]),
                ),
                (r"0123456789name:d:", Some(&["0123456789name", "d:"])),
                (r"MiXeDcAsEnAmE:d:", Some(&["MiXeDcAsEnAmE", "d:"])),
                (r"name:D::rW", Some(&["name", "D:", "rW"])),
                (
                    r"c:/:d:/forward/slashes/are/good/too",
                    Some(&["c:/", "d:/forward/slashes/are/good/too"]),
                ),
                (r"c:\Windows", Some(&[r"c:\Windows"])),
                (
                    r"c:\Program Files (x86)",
                    Some(&[r"c:\Program Files (x86)"]),
                ),
                ("", None),
                (".", Some(&["."])),
                (r"..\", Some(&[r"..\"])),
                (r"c:\:..\", Some(&[r"c:\", r"..\"])),
                (r"c:\:d:\:xyzzy", Some(&[r"c:\", r"d:\", "xyzzy"])),
                ("/tmp/x/y:/foo/x/y", Some(&["/tmp/x/y", "/foo/x/y"])),
            ];
            for (input, expected) in cases {
                let (parsed, _) = parse(input);
                let expected_source = expected.is_some_and(|fields| fields.len() > 1);
                assert_eq!(
                    !parsed.source.is_empty(),
                    expected_source,
                    "Case {input:?}: source={:?}",
                    parsed.source
                );
            }
        }

        // docker_cli_test.go: TestParseWithVolumes — the specs act actually
        // hands to `-v`, and the one field each decides.
        //
        // `parse()` keeps a spec out of `HostConfig.Binds` exactly when
        // `Source` is empty, and strips everything after the second colon when
        // it is a bind. Both are visible here, so both are pinned here.
        #[test]
        fn acts_own_volume_specs_split_into_binds_and_volumes() {
            // A single volume, then two: no source, so no bind.
            for spec in ["/tmp", "/var", "/containerVar"] {
                let volume = expect(spec);
                assert_eq!(volume.mount_type, TYPE_VOLUME, "{spec}");
                assert!(volume.source.is_empty(), "{spec} should have no source");
                assert_eq!(volume.target, spec);
            }

            // A bind, and the same bind with each supported mode spelling.
            // The mode never reaches the bind: `parse()` re-joins the first two
            // fields and drops the rest, so `/hostTmp:/containerTmp:ro` and
            // `/hostTmp:/containerTmp:rw` produce the *same* bind.
            for spec in [
                "/hostTmp:/containerTmp",
                "/hostTmp:/containerTmp:ro",
                "/hostVar:/containerVar:rw",
                "/hostTmp:/containerTmp:ro,Z",
                "/hostVar:/containerVar:rw,Z",
                "/hostTmp:/containerTmp:Z",
                "/hostVar:/containerVar:z",
            ] {
                let volume = expect(spec);
                assert_eq!(volume.mount_type, TYPE_BIND, "{spec}");
                assert_eq!(volume.source, spec.split(':').next().unwrap(), "{spec}");
                assert!(
                    volume.source.starts_with('/'),
                    "{spec} is an absolute path, so it is not rewritten"
                );
            }
            // `:ro` is the one option `Parse` records; `:Z` is not one it knows.
            assert!(expect("/hostTmp:/containerTmp:ro").read_only);
            assert!(!expect("/hostTmp:/containerTmp:rw").read_only);
            assert!(!expect("/hostTmp:/containerTmp:ro,Z")
                .opts
                .is_some_and(|o| matches!(o, MountOpts::Bind { .. })));
        }
    }
}

// ---------------------------------------------------------------------------
// tags.cncf.io/container-device-interface/pkg/parser
// ---------------------------------------------------------------------------

/// Is this `--device` value a CDI qualified name, `vendor.com/class=name`?
///
/// The answer decides the shape of the request: a qualified name becomes a
/// `DeviceRequest` with driver `cdi`, and anything else is a host path that
/// goes through `validateDevice`/`parseDevice`. Getting it wrong means a GPU
/// name is treated as a path, so the rule is ported in full rather than
/// approximated with "does it contain an `=`".
pub mod cdi {
    use std::fmt;

    /// `QualifiedName`: the three parts, joined.
    pub fn qualified_name(vendor: &str, class: &str, name: &str) -> String {
        format!("{vendor}/{class}={name}")
    }

    /// `IsQualifiedName`: the predicate `parse()` branches on.
    pub fn is_qualified_name(device: &str) -> bool {
        parse_qualified_name(device).is_ok()
    }

    /// `ParseQualifiedName`: the three parts, or the reason the value is not a
    /// qualified name.
    ///
    /// Only the *missing vendor* branch is reachable. [`parse_device`] returns
    /// an empty class or an empty name only together with an empty vendor, so
    /// the other two checks can never fire; they are ported because they are
    /// part of the upstream contract and cost two lines.
    pub fn parse_qualified_name(device: &str) -> Result<(String, String, String), String> {
        let (vendor, class, name) = parse_device(device);
        if vendor.is_empty() {
            return Err(format!(
                "unqualified device {}, missing vendor",
                super::go_quote(device)
            ));
        }
        if class.is_empty() {
            return Err(format!(
                "unqualified device {}, missing class",
                super::go_quote(device)
            ));
        }
        if name.is_empty() {
            return Err(format!(
                "unqualified device {}, missing device name",
                super::go_quote(device)
            ));
        }
        validate_vendor_name(&vendor)
            .map_err(|e| format!("invalid device {}: {e}", super::go_quote(device)))?;
        validate_class_name(&class)
            .map_err(|e| format!("invalid device {}: {e}", super::go_quote(device)))?;
        validate_device_name(&name)
            .map_err(|e| format!("invalid device {}: {e}", super::go_quote(device)))?;
        Ok((vendor, class, name))
    }

    /// `ParseDevice`: split without validating.
    ///
    /// A leading `/` short-circuits: `/dev/snd` is a path however much it
    /// resembles a qualified name, and it is what a device *is*.
    pub fn parse_device(device: &str) -> (String, String, String) {
        if device.is_empty() || device.starts_with('/') {
            return (String::new(), String::new(), device.to_string());
        }
        let Some((qualifier, name)) = device.split_once('=') else {
            return (String::new(), String::new(), device.to_string());
        };
        if name.is_empty() {
            return (String::new(), String::new(), device.to_string());
        }
        let (vendor, class) = parse_qualifier(qualifier);
        if vendor.is_empty() {
            return (String::new(), String::new(), device.to_string());
        }
        (vendor, class, name.to_string())
    }

    /// `ParseQualifier`: `vendor/class`.
    pub fn parse_qualifier(kind: &str) -> (String, String) {
        match kind.split_once('/') {
            Some((vendor, class)) if !vendor.is_empty() && !class.is_empty() => {
                (vendor.to_string(), class.to_string())
            }
            _ => (String::new(), kind.to_string()),
        }
    }

    /// `IsLetter`.
    fn is_letter(c: char) -> bool {
        c.is_ascii_alphabetic()
    }

    /// `IsDigit`.
    fn is_digit(c: char) -> bool {
        c.is_ascii_digit()
    }

    /// `IsAlphaNumeric`.
    fn is_alphanumeric(c: char) -> bool {
        is_letter(c) || is_digit(c)
    }

    /// `validateVendorOrClassName`.
    ///
    /// The middle of the name is `name[1..len-1]`, which upstream writes as a Go
    /// byte slice and therefore **panics** for a one-character name. The slice
    /// is clamped instead: see the module header for why, and
    /// [`super::cdi::is_qualified_name`] for what it costs.
    fn validate_vendor_or_class_name(name: &str) -> Result<(), String> {
        if name.is_empty() {
            return Err("empty name".to_string());
        }
        if !is_letter(name.chars().next().expect("checked non-empty")) {
            return Err(format!(
                "{}, should start with letter",
                super::go_quote(name)
            ));
        }
        let chars: Vec<char> = name.chars().collect();
        // A name of one or two characters has no middle, and for a name of one
        // character upstream's `name[1 : len(name)-1]` is `name[1:0]` — see the
        // module header. The guard is the whole deviation.
        if chars.len() > 2 {
            for c in &chars[1..chars.len() - 1] {
                let c = *c;
                if is_alphanumeric(c) {
                    continue;
                }
                if c == '_' || c == '-' || c == '.' {
                    continue;
                }
                return Err(format!(
                    "invalid character '{c}' in name {}",
                    super::go_quote(name)
                ));
            }
        }
        if !is_alphanumeric(chars[chars.len() - 1]) {
            return Err(format!(
                "{} , should end with a letter or digit",
                super::go_quote(name)
            ));
        }
        Ok(())
    }

    /// `ValidateVendorName`.
    pub fn validate_vendor_name(vendor: &str) -> Result<(), String> {
        validate_vendor_or_class_name(vendor).map_err(|e| format!("invalid vendor. {e}"))
    }

    /// `ValidateClassName`.
    pub fn validate_class_name(class: &str) -> Result<(), String> {
        validate_vendor_or_class_name(class).map_err(|e| format!("invalid class. {e}"))
    }

    /// `ValidateDeviceName`.
    ///
    /// Unlike a vendor or a class, a device name may start with a digit and may
    /// contain `:` — which is how `dev_1:2.3` is a legal name. That is also
    /// what lets a path with a colon in it reach the character check instead of
    /// being split, as in `vendor.com/class=de/v`.
    pub fn validate_device_name(name: &str) -> Result<(), String> {
        if name.is_empty() {
            return Err("invalid (empty) device name".to_string());
        }
        let chars: Vec<char> = name.chars().collect();
        if !is_alphanumeric(chars[0]) {
            return Err(format!(
                "invalid class {}, should start with a letter or digit",
                super::go_quote(name)
            ));
        }
        if chars.len() == 1 {
            return Ok(());
        }
        for c in &chars[1..chars.len() - 1] {
            let c = *c;
            if is_alphanumeric(c) {
                continue;
            }
            if c == '_' || c == '-' || c == '.' || c == ':' {
                continue;
            }
            return Err(format!(
                "invalid character '{c}' in device name {}",
                super::go_quote(name)
            ));
        }
        if !is_alphanumeric(chars[chars.len() - 1]) {
            return Err(format!(
                "invalid name {}, should end with a letter or digit",
                super::go_quote(name)
            ));
        }
        Ok(())
    }

    /// A parse failure, rendered the way Go prints it.
    #[derive(Debug)]
    pub struct ParseError(pub String);

    impl fmt::Display for ParseError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.0)
        }
    }

    impl std::error::Error for ParseError {}

    #[cfg(test)]
    mod tests {
        use super::*;

        // parser_test.go: TestQualifiedName
        #[test]
        fn a_qualified_name_splits_into_vendor_class_and_name() {
            // device, vendor, class, name, is_qualified, is_parsable
            #[allow(clippy::type_complexity)]
            let cases: &[(&str, &str, &str, &str, bool, bool)] = &[
                (
                    "vendor.com/class=dev",
                    "vendor.com",
                    "class",
                    "dev",
                    true,
                    false,
                ),
                (
                    "vendor.com/class=0",
                    "vendor.com",
                    "class",
                    "0",
                    true,
                    false,
                ),
                (
                    "vendor1.com/class1=dev1",
                    "vendor1.com",
                    "class1",
                    "dev1",
                    true,
                    false,
                ),
                (
                    "vendor1.com/class.subclass=dev1",
                    "vendor1.com",
                    "class.subclass",
                    "dev1",
                    true,
                    false,
                ),
                (
                    "other-vendor1.com/class_1=dev_1",
                    "other-vendor1.com",
                    "class_1",
                    "dev_1",
                    true,
                    false,
                ),
                (
                    "yet_another-vendor2.com/c-lass_2=dev_1:2.3",
                    "yet_another-vendor2.com",
                    "c-lass_2",
                    "dev_1:2.3",
                    true,
                    false,
                ),
                (
                    "_invalid.com/class=dev",
                    "_invalid.com",
                    "class",
                    "dev",
                    false,
                    true,
                ),
                (
                    "invalid2.com-/class=dev",
                    "invalid2.com-",
                    "class",
                    "dev",
                    false,
                    true,
                ),
                (
                    "invalid3.com/_class=dev",
                    "invalid3.com",
                    "_class",
                    "dev",
                    false,
                    true,
                ),
                (
                    "invalid4.com/class_=dev",
                    "invalid4.com",
                    "class_",
                    "dev",
                    false,
                    true,
                ),
                (
                    "invalid5.com/class=-dev",
                    "invalid5.com",
                    "class",
                    "-dev",
                    false,
                    true,
                ),
                (
                    "invalid6.com/class=dev:",
                    "invalid6.com",
                    "class",
                    "dev:",
                    false,
                    true,
                ),
                ("*.com/*dev=*gpu*", "*.com", "*dev", "*gpu*", false, true),
            ];
            for (device, vendor, class, name, is_qualified, is_parsable) in cases {
                if *is_qualified {
                    assert!(is_qualified_name(device), "qualified name {device:?}");
                    let (v, c, n) =
                        parse_qualified_name(device).unwrap_or_else(|e| panic!("{device}: {e}"));
                    assert_eq!(&v, vendor, "qualified name {device:?}");
                    assert_eq!(&c, class, "qualified name {device:?}");
                    assert_eq!(&n, name, "qualified name {device:?}");

                    let (v, c, n) = parse_device(device);
                    assert_eq!(&v, vendor, "parsed name {device:?}");
                    assert_eq!(&c, class, "parse name {device:?}");
                    assert_eq!(&n, name, "parsed name {device:?}");

                    assert_eq!(
                        qualified_name(&v, &c, &n),
                        *device,
                        "constructed device {device:?}"
                    );
                } else if *is_parsable {
                    assert!(!is_qualified_name(device), "parsed name {device:?}");
                    let (v, c, n) = parse_device(device);
                    assert_eq!(&v, vendor, "parsed name {device:?}");
                    assert_eq!(&c, class, "parse name {device:?}");
                    assert_eq!(&n, name, "parsed name {device:?}");
                }
            }
        }

        // Not upstream. The whole table above happens to avoid a
        // one-character vendor or class, which is exactly where upstream
        // panics; without these the deviation in the module header would be
        // untested.
        #[test]
        fn a_one_character_vendor_or_class_is_a_vendor_not_a_panic() {
            // Upstream: `slice bounds out of range [1:0]` for every one of
            // these, whether the short part is the vendor or the class. What it
            // would have said is still decided by the letter and alphanumeric
            // checks on either side.
            assert!(is_qualified_name("vendor.com/c=d"));
            assert_eq!(
                parse_qualified_name("vendor.com/c=d").expect("a one-letter class"),
                ("vendor.com".into(), "c".into(), "d".into())
            );
            assert_eq!(
                parse_qualified_name("x/_y=z").expect_err("a class must start with a letter"),
                "invalid device \"x/_y=z\": invalid class. \"_y\", should start with letter"
            );
            assert!(is_qualified_name("c/x=d"));
            assert!(is_qualified_name("a/b=c"));
            // A two-character name is the boundary upstream *can* handle.
            assert!(is_qualified_name("vendor.com/cl=d"));
        }

        // Not upstream: the three "missing" branches, which decide that a host
        // path and a malformed qualified name both go to `validateDevice`.
        #[test]
        fn a_host_path_is_never_a_qualified_name() {
            for device in ["/dev/snd", "/dev/kvm", "/dev/dri/card0"] {
                assert!(!is_qualified_name(device), "{device}");
                let err = parse_qualified_name(device).expect_err(device);
                assert_eq!(
                    err,
                    format!(
                        "unqualified device {}, missing vendor",
                        super::super::go_quote(device)
                    )
                );
                // The verbatim input comes back as the "name", so the caller
                // can still see what it was handed.
                assert_eq!(parse_device(device), ("".into(), "".into(), device.into()));
            }
            assert_eq!(
                parse_qualified_name("").expect_err("empty"),
                "unqualified device \"\", missing vendor"
            );
            // A trailing `=` makes `ParseDevice` hand the *whole spec* back as
            // the name, with no vendor and no class — so the class and
            // name branches of `ParseQualifiedName` are unreachable upstream,
            // and both of these report a missing *vendor*.
            for spec in ["vendor.com=", "vendor.com/class=", "vendor.com/class"] {
                assert_eq!(
                    parse_qualified_name(spec).expect_err(spec),
                    format!(
                        "unqualified device {}, missing vendor",
                        super::super::go_quote(spec)
                    )
                );
                assert_eq!(parse_device(spec), ("".into(), "".into(), spec.into()));
            }
        }

        // Not upstream: the character-level messages, which is what a typo in a
        // workflow's `--device` actually surfaces.
        #[test]
        fn an_invalid_character_is_named() {
            assert_eq!(
                parse_qualified_name("1/x=y").expect_err("a digit cannot start a vendor"),
                "invalid device \"1/x=y\": invalid vendor. \"1\", should start with letter"
            );
            assert_eq!(
                parse_qualified_name("vendor.com/class=de/v").expect_err("a slash in the name"),
                "invalid device \"vendor.com/class=de/v\": invalid character '/' in device name \"de/v\""
            );
            assert_eq!(
                parse_qualified_name("x.c/x-y=dev.").expect_err("a trailing dot"),
                "invalid device \"x.c/x-y=dev.\": invalid name \"dev.\", should end with a letter or digit"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The Go stdlib shims, against values read off go1.26.2. They are
    // load-bearing for the four grammars' error text, and none of them is
    // tested upstream because upstream tests them in the standard library.
    #[test]
    fn a_dotted_quad_needs_no_leading_zeros_and_no_surrounding_space() {
        for good in ["0.0.0.0", "192.168.1.100", "255.255.255.255"] {
            assert!(go_parse_ip(good).is_some(), "{good}");
        }
        for good in ["::1", "::", "2001:4860:0:2001::68", "::ffff:1.2.3.4"] {
            assert!(go_parse_ip(good).is_some(), "{good}");
        }
        for bad in [
            "localhost",
            "010.1.1.1",
            "1.2.3.4.5",
            "fe80::1%eth0",
            "",
            "1.2.3.04",
            "1.2.3.4 ",
            " 1.2.3.4",
            "[::1]",
            "1.2.3.4/24",
        ] {
            assert!(go_parse_ip(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_bracketed_host_is_unwrapped_and_a_malformed_one_is_named() {
        assert_eq!(
            go_split_host_port("[2001:4860:0:2001::68]:").expect("bracketed"),
            ("2001:4860:0:2001::68".into(), "".into())
        );
        // An empty bracket is a valid, empty host.
        assert_eq!(
            go_split_host_port("[]:").expect("empty host"),
            ("".into(), "".into())
        );
        assert_eq!(
            go_split_host_port("1.2.3.4:").expect("dotted quad"),
            ("1.2.3.4".into(), "".into())
        );
        for (input, expected) in [
            ("[::1:", "address [::1:: missing ']' in address"),
            ("::1:", "address ::1:: too many colons in address"),
            ("]:", "address ]:: unexpected ']' in address"),
        ] {
            assert_eq!(go_split_host_port(input).expect_err(input), expected);
        }
    }

    #[test]
    fn only_a_host_with_a_colon_gets_bracketed() {
        for (host, port, expected) in [
            ("", "8080:6000/tcp", ":8080:6000/tcp"),
            (
                "192.168.1.100",
                "8080:6000/udp",
                "192.168.1.100:8080:6000/udp",
            ),
            ("::1", "80:80/tcp", "[::1]:80:80/tcp"),
            ("", ":8080:6000/tcp", "::8080:6000/tcp"),
            ("192.168.1.100", ":6000/tcp", "192.168.1.100::6000/tcp"),
            ("::", ":80/tcp", "[::]::80/tcp"),
        ] {
            assert_eq!(go_join_host_port(host, port), expected);
        }
    }

    #[test]
    fn strconv_quotes_like_go() {
        for (input, expected) in [
            ("asdf", "\"asdf\""),
            ("", "\"\""),
            ("1\n", "\"1\\n\""),
            ("a\"b", "\"a\\\"b\""),
            ("a\\b", "\"a\\\\b\""),
            ("\u{7}", "\"\\a\""),
            ("\u{1f}", "\"\\x1f\""),
            ("\u{7f}", "\"\\x7f\""),
        ] {
            assert_eq!(go_quote(input), expected);
        }
    }
}
