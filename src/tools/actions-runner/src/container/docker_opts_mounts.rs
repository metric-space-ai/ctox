//! `--mount`, `--network` and `--gpus`: the three options whose value is a
//! *list* of structured things rather than a single string.
//!
//! Upstream is `github.com/docker/cli`'s `opts/mount.go`,
//! `opts/mount_utils.go`, `opts/network.go` and `opts/gpus.go`. act borrows
//! those three types directly in `pkg/container/docker_run.go`, so their
//! behaviour is act's behaviour and is ported rather than redesigned.
//!
//! # All three parse with `encoding/csv`, and that is load-bearing
//!
//! `--mount`, `--network` and `--gpus` all open a `csv.Reader` on their value
//! and read **one record**. That is why
//! `--gpus 'driver=nvidia,"capabilities=compute,utility"'` works: without the
//! CSV layer the outer comma would split the inner list in two. The same
//! reader produces quoting errors the tests do not cover but a user can hit,
//! so the private `csv` module is a faithful port of `readRecord` rather
//! than `split(',')`.
//!
//! Quoting is per **field**, not per value, which catches people out. To give
//! a `--mount` source a comma the whole field has to be quoted —
//! `type=bind,"source=a,b",target=/c` — because `source="a,b"` leaves the
//! first field as `source="a`, and a `"` inside a non-quoted field is
//! `parse error on line 1, column 18: bare " in non-quoted-field`. Verified
//! against go1.26.2.
//!
//! # An unknown `type=` is accepted, and that is deliberate
//!
//! `type=bogus` parses successfully and is handed to the daemon, which
//! rejects it. `MountOpt::set` only *stores* the type — the cross-field
//! checks in [`mount::validate_mount_options`] ask whether the type is
//! consistent with the option families that were supplied, never whether the
//! type is one it knows. That is why [`mount::MountType`] is a string newtype
//! and not an enum: an
//! enum would have to reject `bogus` here and change behaviour.
//!
//! # `volume-label=bar` is a label with an empty value, not an error
//!
//! This is the one that reads wrong on the first pass. `--mount
//! type=volume,volume-label=bar` looks like a label option that forgot its
//! `=value`, and one would expect the "value is empty" rejection that
//! `--mount type=volume,source=` gets. It does not get it, because the outer
//! field really does have a value — `"bar"` — so it clears the emptiness
//! check. The value is then re-split by [`mount::set_value_on_map`], where a
//! key with no `=` yields an **empty value**, not a missing key. So `bar`
//! becomes a label whose value is `""`. Verified against go1.26.2.
//!
//! The neighbouring `volume-label==foo-value` *is* dropped, and for a
//! different reason: [`mount::set_value_on_map`] cuts on the first `=`, so the
//! key is empty, and an empty key returns the map untouched. The option is
//! silently discarded while `VolumeOptions` is still created — upstream flags
//! this with `TODO(thaJeztah): this should probably be an error instead`.
//!
//! # `bind-recursive=enabled` is the one value that leaves no trace
//!
//! The other three values (`disabled`, `writable`, `readonly`) each call
//! `ensureBindOptions`, which allocates the struct. `enabled` does nothing at
//! all — it is a NOP — so `BindOptions` stays `nil` and the mount serialises
//! exactly as a bind mount with no bind options. [`MountOpt::set`] preserves
//! that: the field is `None`, not `Some(BindOptions::default())`.
//!
//! # Where the fields are `Option` and why they cannot be an enum
//!
//! Go carries five independent `*Options` pointers on `mount.Mount`, and
//! `validateExclusiveOptions` rejects the states where more than one is set.
//! An enum would be tidier, but the tests observe a distinction it cannot
//! express: `volume-label==foo-value` yields a **non-nil but empty**
//! `VolumeOptions`, while a mount with no `volume-*` option at all yields a
//! **nil** one. `Option<VolumeOptions>` keeps both; a unit variant cannot.
//!
//! # `NetworkOpt::network_mode` is not `Value`
//!
//! It reads only the *first* option's target and returns the literal
//! `"default"` when there are none. `--network` is documented as repeatable
//! but only the first entry ever reaches a `HostConfig`, so the rest of the
//! list is parsed, validated, and then discarded.
//!
//! # What is here, and what is still missing
//!
//! The three flag value types are complete against upstream's own tables —
//! [`MountOpt`] (all twelve tests in `mount_test.go`), [`NetworkOpt`] and
//! [`GpuOpts`] — and each one has `set`, `value`, its `Type()` name and its
//! `String()` rendering, which is the surface `pflags` needs to register a
//! flag. The CSV reader is exercised directly for the quoting cases the three
//! tables do not reach.
//!
//! They parse, though, and nothing consumes them yet:
//!
//! * **`parse()` — the function that assembles a `HostConfig` — does not
//!   exist.** Nothing copies a [`mount::Mount`], a
//!   [`NetworkAttachmentOpts`] or a [`DeviceRequest`] onto a container, and
//!   `docker_specs` has no `HostConfig` shape to copy them into yet.
//! * **`opts.ParseMountRaw` is not ported, because it does not exist at this
//!   version.** It was the `-v`/`--volume` parser; `docker/cli` v29.3.0 has no
//!   such symbol (grepped the module), so a port would have to find where
//!   `--volume` is parsed now. It never fed [`MountOpt`], so nothing above
//!   depends on it.
//! * Two upstream error texts are approximated rather than reproduced, both
//!   untested upstream: `netip.ParseAddr`'s reason text
//!   (the `go_parse_addr` helper) and Go's `%v` of a `DeviceRequest`
//!   (the `go_value_v` helper).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::OnceLock;

use regex::Regex;

use super::docker_api::DeviceRequest;

// ---------------------------------------------------------------------------
// encoding/csv
// ---------------------------------------------------------------------------

/// `encoding/csv`'s `Reader.Read`, narrowed to the one record these three
/// options ever take.
///
/// `LazyQuotes`, `TrimLeadingSpace` and `Comment` are all left at their zero
/// values, exactly as the three `csv.NewReader(...)` calls upstream do, and the
/// delimiter is `,`. Errors are returned pre-formatted in Go's `ParseError`
/// wording because that wording is what a user sees:
///
/// ```text
/// parse error on line 1, column 4: bare " in non-quoted-field
/// ```
mod csv {
    /// `bufio.Reader.ReadSlice('\n')` over an in-memory string.
    struct Cursor<'a> {
        data: &'a [u8],
        pos: usize,
    }

    impl<'a> Cursor<'a> {
        /// One `readLine`: bytes up to and including the `\n`, `\r\n`
        /// normalised to `\n`, and `eof` set only when the reader is *spent*.
        ///
        /// "Spent" is not the same as "a line without a trailing newline".
        /// `bufio.Reader.ReadSlice` reports `io.EOF` only when it produced no
        /// bytes at all; a final unterminated line comes back with a `nil`
        /// error. Since no `--mount`, `--network` or `--gpus` value arrives
        /// with a trailing newline, conflating the two would turn every value
        /// of all three flags into an `EOF` error.
        ///
        /// Go returns the bytes *with* the `\r` on the EOF case and then drops
        /// it for backwards compatibility, which is the same as dropping it
        /// here.
        fn read_line(&mut self) -> (Vec<u8>, bool) {
            if self.pos >= self.data.len() {
                return (Vec::new(), true);
            }
            let start = self.pos;
            let end = match self.data[start..].iter().position(|&b| b == b'\n') {
                Some(offset) => start + offset + 1,
                None => self.data.len(),
            };
            self.pos = end;
            let mut line = self.data[start..end].to_vec();
            if !line.contains(&b'\n') && line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.len() >= 2 && line[line.len() - 2] == b'\r' && *line.last().unwrap() == b'\n' {
                let last = line.len() - 1;
                line[last - 1] = b'\n';
                line.pop();
            }
            (line, false)
        }
    }

    /// `lengthNL`: 1 for a trailing newline, else 0.
    fn length_nl(line: &[u8]) -> usize {
        usize::from(line.last() == Some(&b'\n'))
    }

    /// `ParseError.Error`, for the `ErrBareQuote` / `ErrQuote` cases only.
    fn parse_error(start_line: usize, line: usize, column: usize, message: &str) -> String {
        if start_line != line {
            return format!(
                "record on line {start_line}; parse error on line {line}, column {column}: {message}"
            );
        }
        format!("parse error on line {line}, column {column}: {message}")
    }

    /// One `readRecord`. `Err` carries `"EOF"` for an exhausted reader, which
    /// is what `gpus.Set("")` returns upstream.
    pub(super) fn read_record(input: &str) -> Result<Vec<String>, String> {
        let mut cursor = Cursor {
            data: input.as_bytes(),
            pos: 0,
        };
        let mut num_line = 0usize;
        let mut line: Vec<u8>;
        let mut err_read: bool;
        // Read one line, skipping blank ones. Upstream's loop condition is
        // `errRead == nil`, so a blank line at EOF falls through to the same
        // EOF return as an empty input.
        loop {
            let (read, eof) = cursor.read_line();
            num_line += 1;
            line = read;
            err_read = eof;
            if !err_read && line.len() == length_nl(&line) {
                continue;
            }
            break;
        }
        if err_read {
            return Err("EOF".to_string());
        }

        // Fields are cut out of one shared buffer, so `pos` tracks a column
        // within the record rather than within the current field.
        let rec_line = num_line;
        let (mut col, mut cur_line) = (1usize, num_line);
        let mut record: Vec<u8> = Vec::new();
        let mut field_ends: Vec<usize> = Vec::new();

        'fields: loop {
            if line.is_empty() || line[0] != b'"' {
                // Non-quoted field: up to the next comma or end of line.
                let comma = line.iter().position(|&b| b == b',');
                let stop = comma.unwrap_or(line.len() - length_nl(&line));
                let field = &line[..stop];
                if let Some(offset) = field.iter().position(|&b| b == b'"') {
                    return Err(parse_error(
                        rec_line,
                        cur_line,
                        col + offset,
                        "bare \" in non-quoted-field",
                    ));
                }
                record.extend_from_slice(field);
                field_ends.push(record.len());
                match comma {
                    Some(offset) => {
                        line = line[offset + 1..].to_vec();
                        col += offset + 1;
                    }
                    None => break 'fields,
                }
            } else {
                // Quoted field. Its errors report the column of the offending
                // quote minus its own length, which is why the `col` bookkeeping
                // below is not simply "bytes consumed".
                line.drain(..1);
                col += 1;
                loop {
                    match line.iter().position(|&b| b == b'"') {
                        Some(offset) => {
                            record.extend_from_slice(&line[..offset]);
                            line.drain(..offset + 1);
                            col += offset + 1;
                            match line.first() {
                                // `""` — an escaped quote.
                                Some(b'"') => {
                                    record.push(b'"');
                                    line.drain(..1);
                                    col += 1;
                                }
                                // `",` — end of field.
                                Some(b',') => {
                                    line.drain(..1);
                                    col += 1;
                                    field_ends.push(record.len());
                                    continue 'fields;
                                }
                                // `"\n` — end of field at end of line.
                                _ if length_nl(&line) == line.len() => {
                                    field_ends.push(record.len());
                                    break 'fields;
                                }
                                // `"*` — a quote that escapes nothing.
                                _ => {
                                    return Err(parse_error(
                                        rec_line,
                                        cur_line,
                                        col - 1,
                                        "extraneous or missing \" in quoted-field",
                                    ));
                                }
                            }
                        }
                        None => {
                            if !line.is_empty() {
                                // A quoted field may span lines: the whole line
                                // is copied in and the reader is asked for
                                // another, without a delimiter in sight. This
                                // is what lets a `--mount` or `--gpus` value
                                // carry a newline inside one option.
                                record.extend_from_slice(&line);
                                col += line.len();
                                let (read, eof) = cursor.read_line();
                                if !read.is_empty() {
                                    // `pos.line`, not `r.numLine`: the line
                                    // counter a `ParseError` reports is the one
                                    // that moves only when a line actually
                                    // carries bytes.
                                    cur_line += 1;
                                    col = 1;
                                }
                                line = read;
                                if eof {
                                    // Go normalises `io.EOF` back to a `nil`
                                    // read error here, so the loop below reports
                                    // an unterminated quoted field rather than
                                    // accepting the record.
                                    err_read = false;
                                }
                            } else if !err_read {
                                return Err(parse_error(
                                    rec_line,
                                    cur_line,
                                    col,
                                    "extraneous or missing \" in quoted-field",
                                ));
                            } else {
                                field_ends.push(record.len());
                                break 'fields;
                            }
                        }
                    }
                }
            }
        }

        let mut fields = Vec::with_capacity(field_ends.len());
        let mut start = 0usize;
        for end in field_ends {
            fields.push(String::from_utf8_lossy(&record[start..end]).into_owned());
            start = end;
        }
        Ok(fields)
    }
}

// ---------------------------------------------------------------------------
// go-units / strconv helpers
// ---------------------------------------------------------------------------

/// `strconv.Atoi`, reduced to the reason text the two callers report.
///
/// Upstream unwraps `*strconv.NumError` down to its `Err`, so callers see the
/// bare `"invalid syntax"` / `"value out of range"` rather than the
/// `strconv.Atoi: parsing "…": …` wrapper.
fn go_atoi(value: &str) -> Result<i64, &'static str> {
    let bytes = value.as_bytes();
    let Some(&first) = bytes.first() else {
        return Err("invalid syntax");
    };
    let (sign, digits) = match first {
        b'+' => (1i64, &bytes[1..]),
        b'-' => (-1i64, &bytes[1..]),
        _ => (1i64, bytes),
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return Err("invalid syntax");
    }
    let Ok(magnitude) = std::str::from_utf8(digits).unwrap_or("").parse::<i64>() else {
        return Err("value out of range");
    };
    Ok(sign * magnitude)
}

/// `strconv.ParseUint(value, 8, 32)`, used by `tmpfs-mode`.
///
/// An explicit base means Go accepts neither a `0o`/`0x` prefix nor a sign,
/// so `0o777` is a syntax error while both `777` and `0777` are 511. Out of
/// range is an error too, which the caller reports the same way as a syntax
/// error.
pub fn go_parse_uint_octal_32(value: &str) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return None;
    }
    u32::from_str_radix(value, 8).ok()
}

/// `go-units.RAMInBytes`, the `binaryMap` branch of `parseSize`.
///
/// `tmpfs-size` is the only caller and it discards the reason, so this needs to
/// agree with Go on the *value*, not the wording. The scan is
/// `LastIndexAny(size, "01234567890. ")` — note that a trailing space is a
/// separator, which is why `"1 m"` parses as one megabyte, and that the empty
/// string and anything with no digit are rejected rather than treated as zero.
fn ram_in_bytes(value: &str) -> Result<i64, String> {
    const BINARY: [(u8, i64); 5] = [
        (b'k', 1024),
        (b'm', 1024 * 1024),
        (b'g', 1024 * 1024 * 1024),
        (b't', 1024i64 * 1024 * 1024 * 1024),
        (b'p', 1024i64 * 1024 * 1024 * 1024 * 1024),
    ];

    let invalid = || format!("invalid size: '{value}'");
    // `LastIndexAny` over digits, `.` and a space, over *bytes* — Go indexes
    // the string, not runes.
    let Some(sep) = value
        .as_bytes()
        .iter()
        .rposition(|b| b.is_ascii_digit() || *b == b'.' || *b == b' ')
    else {
        return Err(invalid());
    };
    let (number, suffix) = if value.as_bytes()[sep] == b' ' {
        (&value[..sep], &value[sep + 1..])
    } else {
        (&value[..sep + 1], &value[sep + 1..])
    };

    let Ok(size) = number.parse::<f64>() else {
        return Err(invalid());
    };
    // Backward compatibility: negative sizes are rejected even though the
    // suffix logic below would not mind them.
    if size < 0.0 {
        return Err(invalid());
    }
    if suffix.is_empty() {
        return Ok(size as i64);
    }

    let bad_suffix = || format!("invalid suffix: '{suffix}'");
    if suffix.len() > 3 {
        return Err(bad_suffix());
    }
    let lowered = suffix.to_lowercase();
    let unit = lowered.as_bytes()[0];
    // A bare `b` means bytes, and nothing may follow it.
    if unit == b'b' {
        return if lowered.len() > 1 {
            Err(bad_suffix())
        } else {
            Ok(size as i64)
        };
    }
    let Some(&(_, multiplier)) = BINARY.iter().find(|(name, _)| *name == unit) else {
        return Err(bad_suffix());
    };
    // The only suffixes past the unit letter are `b` and `ib`, so `KiB` and `K`
    // both work while `Kib` does not.
    let tail = &lowered[1..];
    if (tail.len() == 1 && tail != "b") || (tail.len() == 2 && tail != "ib") {
        return Err(bad_suffix());
    }
    Ok((size * multiplier as f64) as i64)
}

/// Go's `%q` for the one error message that interpolates a user value.
///
/// `parseBoolValue` embeds the rejected value this way, and the test asserts
/// on the result, so the quoting has to be Go's and not Rust's `Debug`.
pub fn go_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `filepath.Abs` on a unix host, which is `Clean(Join(wd, path))`.
///
/// `MountOpt::set` reaches for this only when a source starts with `.` and is
/// not already absolute, which is why `--mount type=bind,src=./x` and
/// `--mount type=bind,src=x` behave differently.
fn go_filepath_abs(path: &str) -> Option<String> {
    let cleaned = if super::docker_opts::slash_is_abs(path) {
        super::docker_opts::slash_clean(path)
    } else {
        let wd = std::env::current_dir().ok()?;
        super::docker_opts::slash_clean(&format!("{}/{}", wd.to_string_lossy(), path))
    };
    Some(cleaned)
}

/// `netip.ParseAddr`, narrowed to the message `--network`'s `ip=`, `ip6=` and
/// `link-local-ip=` produce.
///
/// Two divergences from `netip`, both untested upstream and both about input
/// the daemon would reject anyway: an IPv6 zone (`fe80::1%eth0`) parses in Go
/// and not here, and Go's reason text is more specific than this one
/// (`ParseAddr("1.2.3"): IPv4 address too short` where this always says
/// `unable to parse IP`). The wrapping — `ParseAddr(%q): …` — is kept, because
/// that is what a user reads.
fn go_parse_addr(value: &str) -> Result<std::net::IpAddr, String> {
    value
        .parse::<std::net::IpAddr>()
        .map_err(|_| format!("ParseAddr({}): unable to parse IP", go_quote(value)))
}

// ---------------------------------------------------------------------------
// mount types
// ---------------------------------------------------------------------------

/// `github.com/moby/moby/api/types/mount`.
///
/// The types `mount.Mount` is built from, plus the client-side validation
/// that turns a half-built mount into an error message.
pub mod mount {
    use serde::Serialize;
    use std::collections::BTreeMap;
    use std::fmt;

    use super::{go_quote, ram_in_bytes};

    /// `mount.TypeBind`.
    pub const TYPE_BIND: &str = "bind";
    /// `mount.TypeVolume`.
    pub const TYPE_VOLUME: &str = "volume";
    /// `mount.TypeTmpfs`.
    pub const TYPE_TMPFS: &str = "tmpfs";
    /// `mount.TypeNamedPipe`: Windows named pipes.
    pub const TYPE_NAMED_PIPE: &str = "npipe";
    /// `mount.TypeCluster`.
    pub const TYPE_CLUSTER: &str = "cluster";
    /// `mount.TypeImage`.
    pub const TYPE_IMAGE: &str = "image";

    /// `mount.Propagations`, the option values that name a bind propagation.
    pub const PROPAGATIONS: &[&str] =
        &["rprivate", "private", "rshared", "shared", "rslave", "slave"];

    /// `mount.Type`: the mount type as a **string**, not an enum.
    ///
    /// [`super::MountOpt::set`] accepts any value here and defers the rejection
    /// to the daemon, so this cannot be a closed set without changing behaviour.
    /// Use
    /// the `TYPE_*` constants, or [`MountType::is_bind`] and friends, to compare.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
    #[serde(transparent)]
    pub struct MountType(pub String);

    impl MountType {
        /// A host directory.
        pub fn is_bind(&self) -> bool {
            self.0 == TYPE_BIND
        }

        /// A named volume, or an anonymous one.
        pub fn is_volume(&self) -> bool {
            self.0 == TYPE_VOLUME
        }

        /// An in-memory filesystem.
        pub fn is_tmpfs(&self) -> bool {
            self.0 == TYPE_TMPFS
        }

        /// A Windows named pipe.
        pub fn is_named_pipe(&self) -> bool {
            self.0 == TYPE_NAMED_PIPE
        }

        /// Another image's filesystem.
        pub fn is_image(&self) -> bool {
            self.0 == TYPE_IMAGE
        }
    }

    impl fmt::Display for MountType {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.0)
        }
    }

    /// `mount.Propagation`: how a bind mount propagates to its submounts.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
    #[serde(transparent)]
    pub struct Propagation(pub String);

    impl Propagation {
        /// `mount.PropagationRPrivate`, the default, and the only propagation
        /// `bind-recursive=readonly` accepts.
        pub fn is_rprivate(&self) -> bool {
            self.0 == "rprivate"
        }

        /// Whether the string names a propagation at all. Never used by
        /// `MountOpt::set`, which stores whatever it is given.
        pub fn is_known(&self) -> bool {
            PROPAGATIONS.contains(&self.0.as_str())
        }
    }

    /// `mount.Consistency`: the `consistency=` option.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
    #[serde(transparent)]
    pub struct Consistency(pub String);

    /// `mount.BindOptions`.
    ///
    /// `None` on a [`Mount`] means no `bind-*` option was given at all, which
    /// is a state the tests assert on directly and which
    /// `bind-recursive=enabled` leaves behind.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
    pub struct BindOptions {
        /// `bind-propagation=`.
        #[serde(rename = "Propagation")]
        pub propagation: Propagation,
        /// `bind-recursive=disabled`, the old `bind-nonrecursive=true`.
        #[serde(rename = "NonRecursive")]
        pub non_recursive: bool,
        /// `bind-create-src`.
        #[serde(rename = "CreateMountpoint")]
        pub create_mountpoint: bool,
        /// `bind-recursive=writable`: recursive, but not recursively read-only.
        #[serde(rename = "ReadOnlyNonRecursive")]
        pub read_only_non_recursive: bool,
        /// `bind-recursive=readonly`: fail unless the mount can be made
        /// recursively read-only.
        #[serde(rename = "ReadOnlyForceRecursive")]
        pub read_only_force_recursive: bool,
    }

    /// `mount.Driver`: a volume driver name and its options.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
    pub struct Driver {
        /// `volume-driver=`.
        #[serde(rename = "Name")]
        pub name: String,
        /// `volume-opt=`.
        #[serde(rename = "Options")]
        pub options: Option<BTreeMap<String, String>>,
    }

    /// `mount.VolumeOptions`.
    ///
    /// `None` when no `volume-*` option was given; `Some` but all-default when
    /// one was given and turned out to carry an empty key.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
    pub struct VolumeOptions {
        /// `volume-nocopy`.
        #[serde(rename = "NoCopy")]
        pub no_copy: bool,
        /// `volume-label=`.
        #[serde(rename = "Labels")]
        pub labels: Option<BTreeMap<String, String>>,
        /// `volume-subpath=`.
        #[serde(rename = "Subpath")]
        pub subpath: String,
        /// `volume-driver=` and `volume-opt=`.
        #[serde(rename = "DriverConfig")]
        pub driver_config: Option<Driver>,
    }

    /// `mount.ImageOptions`.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
    pub struct ImageOptions {
        /// `image-subpath=`.
        #[serde(rename = "Subpath")]
        pub subpath: String,
    }

    /// `mount.TmpfsOptions`.
    ///
    /// [`TmpfsOptions::options`] is always empty: `MountOpt::set` has no
    /// `tmpfs-opt` key, and the daemon-side flags upstream lists as possible
    /// (`uid`, `gid`, `nr_inodes`, …) have no client-side spelling. The field
    /// is kept so the struct still round-trips the wire type.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
    pub struct TmpfsOptions {
        /// `tmpfs-size=`, in bytes.
        #[serde(rename = "SizeBytes")]
        pub size_bytes: i64,
        /// `tmpfs-mode=`, as a file mode. Octal digits only: `700` and `0777`
        /// are the same number, and `0o700` is a syntax error.
        #[serde(rename = "Mode")]
        pub mode: u32,
        /// Never populated by `MountOpt::set`.
        #[serde(rename = "Options")]
        pub options: Vec<Vec<String>>,
    }

    /// `mount.ClusterOptions`, which is an intentionally empty struct upstream.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
    pub struct ClusterOptions;

    /// `mount.Mount`: one `--mount` flag's result.
    #[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
    pub struct Mount {
        /// `type=`. Defaults to [`TYPE_VOLUME`].
        #[serde(rename = "Type")]
        pub mount_type: MountType,
        /// `source=` / `src=`: a host path or a volume name.
        #[serde(rename = "Source")]
        pub source: String,
        /// `target=` / `dst=` / `destination=`.
        #[serde(rename = "Target")]
        pub target: String,
        /// `readonly` / `ro`.
        #[serde(rename = "ReadOnly")]
        pub read_only: bool,
        /// `consistency=`.
        #[serde(rename = "Consistency")]
        pub consistency: Consistency,
        /// The `bind-*` options, if any.
        #[serde(rename = "BindOptions")]
        pub bind_options: Option<BindOptions>,
        /// The `volume-*` options, if any.
        #[serde(rename = "VolumeOptions")]
        pub volume_options: Option<VolumeOptions>,
        /// The `image-*` options, if any.
        #[serde(rename = "ImageOptions")]
        pub image_options: Option<ImageOptions>,
        /// The `tmpfs-*` options, if any.
        #[serde(rename = "TmpfsOptions")]
        pub tmpfs_options: Option<TmpfsOptions>,
        /// The `cluster-*` options. Unreachable from `MountOpt::set`, which has
        /// no `cluster-` key — `--mount type=cluster,cluster=x` fails with
        /// `unknown option 'cluster' in 'cluster=x'`. Kept so
        /// [`validate_exclusive`] can state the same rule the daemon
        /// states.
        #[serde(rename = "ClusterOptions")]
        pub cluster_options: Option<ClusterOptions>,
    }

    /// `validateExclusiveOptions`: only the current type's option family may
    /// be present.
    ///
    /// The order is the behaviour. It is bind, volume, image, tmpfs, cluster,
    /// and the first mismatch wins — so `--mount type=volume,volume-nocopy=true,
    /// bind-propagation=rprivate` reports the *bind* complaint, even though the
    /// volume option came first in the flag.
    ///
    /// The `"type is required"` branch is dead from `set`, which defaults the
    /// type to `volume` before parsing a single field, and `type=` with an empty
    /// value is rejected earlier. It is here because it is upstream's rule.
    pub fn validate_exclusive(m: &Mount) -> Result<(), String> {
        if m.mount_type.0.is_empty() {
            return Err("type is required".to_string());
        }
        let type_name = &m.mount_type.0;
        if !m.mount_type.is_bind() && m.bind_options.is_some() {
            return Err(format!(
                "cannot mix 'bind-*' options with mount type '{type_name}'"
            ));
        }
        if !m.mount_type.is_volume() && m.volume_options.is_some() {
            return Err(format!(
                "cannot mix 'volume-*' options with mount type '{type_name}'"
            ));
        }
        if !m.mount_type.is_image() && m.image_options.is_some() {
            return Err(format!(
                "cannot mix 'image-*' options with mount type '{type_name}'"
            ));
        }
        if !m.mount_type.is_tmpfs() && m.tmpfs_options.is_some() {
            return Err(format!(
                "cannot mix 'tmpfs-*' options with mount type '{type_name}'"
            ));
        }
        if type_name != TYPE_CLUSTER && m.cluster_options.is_some() {
            return Err(format!(
                "cannot mix 'cluster-*' options with mount type '{type_name}'"
            ));
        }
        Ok(())
    }

    /// `validateMountOptions`: the checks that need more than one field.
    ///
    /// The two `bind-recursive` rules are the whole function, and both are
    /// reachability constraints rather than syntax checks:
    /// `writable` is meaningless without `readonly`, and `readonly` is
    /// meaningless — and unimplementable — without `bind-propagation=rprivate`.
    /// The daemon does **not** re-check the second one, which is what the
    /// upstream `FIXME(thaJeztah)` records.
    pub fn validate_mount_options(m: &Mount) -> Result<(), String> {
        validate_exclusive(m)?;
        let Some(bind) = &m.bind_options else {
            return Ok(());
        };
        if bind.read_only_non_recursive && !m.read_only {
            return Err(
                "option 'bind-recursive=writable' requires 'readonly' to be specified in conjunction"
                    .to_string(),
            );
        }
        if bind.read_only_force_recursive {
            if !m.read_only {
                return Err("option 'bind-recursive=readonly' requires 'readonly' to be specified in conjunction".to_string());
            }
            if !bind.propagation.is_rprivate() {
                return Err("option 'bind-recursive=readonly' requires 'bind-propagation=rprivate' to be specified in conjunction".to_string());
            }
        }
        Ok(())
    }

    /// `parseBoolValue`: `1`/`true`/`0`/`false`, and a bare key means `true`.
    ///
    /// Deliberately not `strconv.ParseBool`, which also accepts `t`, `T`, `TRUE`
    /// and `False`. The rejection message is asserted verbatim upstream, so the
    /// `%q` quoting and the trailing `(default "true")` are both load-bearing.
    pub fn parse_bool_value(key: &str, value: &str, has_value: bool) -> Result<bool, String> {
        if !has_value {
            return Ok(true);
        }
        match value {
            "1" | "true" => Ok(true),
            "0" | "false" => Ok(false),
            other => Err(format!(
                "invalid value for '{key}': invalid boolean value ({}): must be one of \"true\", \"1\", \"false\", or \"0\" (default \"true\")",
                go_quote(other)
            )),
        }
    }

    /// `setValueOnMap`: `k=v` onto a map that may still be `None`.
    ///
    /// Two behaviours are worth stating outright. A key with no `=` gets the
    /// **empty value** rather than being dropped — that is why
    /// `volume-label=bar` yields `{"bar": ""}`. An **empty** key drops the pair
    /// entirely and returns the map untouched — that is why
    /// `volume-label==foo-value` adds nothing while still creating
    /// `VolumeOptions`.
    pub fn set_value_on_map(
        target: Option<BTreeMap<String, String>>,
        key_value: &str,
    ) -> Option<BTreeMap<String, String>> {
        // `strings.Cut` semantics again: absent `=` yields the whole string as
        // the key and an empty value, and an empty key drops the pair.
        let (key, value) = match key_value.split_once('=') {
            Some((key, value)) => (key, value),
            None => (key_value, ""),
        };
        if key.is_empty() {
            return target;
        }
        let mut map = target.unwrap_or_default();
        map.insert(key.to_string(), value.to_string());
        Some(map)
    }

    /// The `tmpfs-size` conversion, exposed so its Go provenance stays next to
    /// the only caller.
    pub fn tmpfs_size_bytes(value: &str) -> Result<i64, String> {
        ram_in_bytes(value)
    }

    /// `MountOpt::Type()`.
    pub const OPTION_NAME: &str = "mount";

    /// Re-exported so a mount parser need not reach past this module.
    pub use super::go_parse_uint_octal_32;
}

// ---------------------------------------------------------------------------
// MountOpt
// ---------------------------------------------------------------------------

/// `ensureBindOptions` / `ensureVolumeOptions` / `ensureVolumeDriver` /
/// `ensureImageOptions` / `ensureTmpfsOptions`: the five `nil` → `&T{}`
/// allocations, without which the *type* of a mount cannot be told from a
/// mount that merely *had* options of that type.
fn ensure_bind_options(m: &mut mount::Mount) -> &mut mount::BindOptions {
    m.bind_options.get_or_insert_with(Default::default)
}

fn ensure_volume_options(m: &mut mount::Mount) -> &mut mount::VolumeOptions {
    m.volume_options.get_or_insert_with(Default::default)
}

/// Go's `ensureVolumeDriver` calls `ensureVolumeOptions` first, so naming a
/// driver *always* creates the volume options too.
fn ensure_volume_driver(m: &mut mount::Mount) -> &mut mount::Driver {
    ensure_volume_options(m)
        .driver_config
        .get_or_insert_with(Default::default)
}

fn ensure_image_options(m: &mut mount::Mount) -> &mut mount::ImageOptions {
    m.image_options.get_or_insert_with(Default::default)
}

fn ensure_tmpfs_options(m: &mut mount::Mount) -> &mut mount::TmpfsOptions {
    m.tmpfs_options.get_or_insert_with(Default::default)
}

/// `opts.MountOpt`: the repeatable `--mount` flag.
///
/// One call to [`MountOpt::set`] is one entry in [`MountOpt::value`], and a
/// rejected value appends nothing — which is what lets a workflow keep the
/// mounts it already accepted when a later one is malformed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MountOpt {
    values: Vec<mount::Mount>,
}

impl MountOpt {
    /// `Type()`.
    pub fn type_name(&self) -> &'static str {
        mount::OPTION_NAME
    }

    /// `Value()`: the mounts parsed so far, in the order they were given.
    pub fn value(&self) -> &[mount::Mount] {
        &self.values
    }

    /// `Set`: one `--mount` value, appended on success.
    ///
    /// The order of the checks is the contract, because each one rejects a
    /// value a later check would have accepted. A field's *key* must not carry
    /// whitespace, then its *value* must be present and unspaced, then the key
    /// is lowercased, and only then is the key matched. So
    /// `type=volume, src=/foo` is a whitespace error rather than an unknown
    /// option, and `type=volume,volume-label==foo-value` survives all three to
    /// be dropped by [`mount::set_value_on_map`].
    pub fn set(&mut self, value: &str) -> Result<(), String> {
        // `strings.TrimSpace` on the whole value only. A field's own padding is
        // still an error, which is the point of the check below.
        let value = value.trim();
        if value.is_empty() {
            return Err("value is empty".to_string());
        }
        let fields = csv::read_record(value)?;

        // The type defaults to volume *before* any field is read, so
        // `target=/target,source=/foo` is a volume mount and `type=` cannot
        // leave the type empty.
        let mut m = mount::Mount {
            mount_type: mount::MountType(mount::TYPE_VOLUME.to_string()),
            ..Default::default()
        };

        for field in fields {
            let (key, val, has_value) = match field.split_once('=') {
                Some((key, val)) => (key, val, true),
                None => (field.as_str(), "", false),
            };
            if key.trim() != key {
                return Err(format!(
                    "invalid option '{}' in '{field}': option should not have whitespace",
                    key.trim()
                ));
            }
            if has_value {
                let trimmed = val.trim();
                if trimmed.is_empty() {
                    return Err(format!("invalid value for '{key}': value is empty"));
                }
                if trimmed != val {
                    return Err(format!(
                        "invalid value for '{key}' in '{field}': value should not have whitespace"
                    ));
                }
            }

            // `TODO(thaJeztah): these options should not be case-insensitive.`
            let key = key.to_lowercase();

            // A bare key is only a flag when it is one of the five boolean-ish
            // options. Checked before the match so that, say, a bare `readonly`
            // reaches `parseBoolValue` as "no value" and a bare `bind-propagation`
            // never does.
            if !has_value
                && !matches!(
                    key.as_str(),
                    "readonly" | "ro" | "volume-nocopy" | "bind-nonrecursive" | "bind-create-src"
                )
            {
                return Err(format!("invalid field '{field}' must be a key=value pair"));
            }

            match key.as_str() {
                "type" => m.mount_type = mount::MountType(val.to_lowercase()),
                "source" | "src" => {
                    m.source = val.to_string();
                    // Only a *relative* path written with a leading dot is
                    // resolved, and a failure to resolve one is not an error:
                    // the un-absolutised value is kept.
                    if !super::docker_opts::slash_is_abs(val) && val.starts_with('.') {
                        if let Some(abs) = go_filepath_abs(val) {
                            m.source = abs;
                        }
                    }
                }
                "target" | "dst" | "destination" => m.target = val.to_string(),
                "readonly" | "ro" => m.read_only = mount::parse_bool_value(&key, val, has_value)?,
                "consistency" => m.consistency = mount::Consistency(val.to_lowercase()),
                "bind-propagation" => {
                    ensure_bind_options(&mut m).propagation = mount::Propagation(val.to_lowercase());
                }
                "bind-nonrecursive" => {
                    return Err(
                        "bind-nonrecursive is deprecated, use bind-recursive=disabled instead"
                            .to_string(),
                    );
                }
                "bind-recursive" => match val {
                    // Read-only mounts are recursively read-only on a new enough
                    // engine, and writable on an old one, so this is the default
                    // expressed as a no-op — and, unlike its three siblings, it
                    // allocates no `BindOptions`.
                    "enabled" => {}
                    "disabled" => ensure_bind_options(&mut m).non_recursive = true,
                    "writable" => ensure_bind_options(&mut m).read_only_non_recursive = true,
                    "readonly" => ensure_bind_options(&mut m).read_only_force_recursive = true,
                    other => {
                        return Err(format!(
                            "invalid value for {key}: {other} (must be \"enabled\", \"disabled\", \"writable\", or \"readonly\")"
                        ))
                    }
                },
                "bind-create-src" => {
                    let create = mount::parse_bool_value(&key, val, has_value)?;
                    ensure_bind_options(&mut m).create_mountpoint = create;
                }
                "volume-subpath" => ensure_volume_options(&mut m).subpath = val.to_string(),
                "volume-nocopy" => {
                    let no_copy = mount::parse_bool_value(&key, val, has_value)?;
                    ensure_volume_options(&mut m).no_copy = no_copy;
                }
                "volume-label" => {
                    let options = ensure_volume_options(&mut m);
                    options.labels = mount::set_value_on_map(options.labels.clone(), val);
                }
                "volume-driver" => ensure_volume_driver(&mut m).name = val.to_string(),
                "volume-opt" => {
                    let driver = ensure_volume_driver(&mut m);
                    driver.options = mount::set_value_on_map(driver.options.clone(), val);
                }
                "image-subpath" => ensure_image_options(&mut m).subpath = val.to_string(),
                "tmpfs-size" => {
                    // The reason `RAMInBytes` gives is discarded upstream, so
                    // the message quotes the value and not the unit problem.
                    let size = ram_in_bytes(val)
                        .map_err(|_| format!("invalid value for {key}: {val}"))?;
                    ensure_tmpfs_options(&mut m).size_bytes = size;
                }
                "tmpfs-mode" => {
                    let mode = go_parse_uint_octal_32(val)
                        .ok_or_else(|| format!("invalid value for {key}: {val}"))?;
                    ensure_tmpfs_options(&mut m).mode = mode;
                }
                _ => return Err(format!("unknown option '{key}' in '{field}'")),
            }
        }

        mount::validate_mount_options(&m)?;
        self.values.push(m);
        Ok(())
    }
}

impl fmt::Display for MountOpt {
    /// `String()`: one `"<type> <source> <target>"` per mount, `", "`-joined.
    ///
    /// A debug rendering, used by `pflag` for a flag's default value, and the
    /// reason it prints all three fields even when two of them are empty.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reprs: Vec<String> = self
            .values
            .iter()
            .map(|m| format!("{} {} {}", m.mount_type, m.source, m.target))
            .collect();
        f.write_str(&reprs.join(", "))
    }
}

// ---------------------------------------------------------------------------
// NetworkOpt
// ---------------------------------------------------------------------------

/// `opts.NetworkAttachmentOpts`: one `--network` value, parsed.
///
/// Go's zero `netip.Addr` is the invalid address; [`Option`] carries that as
/// `None`. The one state Rust cannot carry is `Aliases`: Go's legacy syntax
/// leaves the slice `nil` and the advanced syntax makes it an empty non-nil
/// slice, and both render as `[]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkAttachmentOpts {
    /// `name=`, or the whole value in legacy syntax.
    pub target: String,
    /// `alias=`, repeatable.
    pub aliases: Vec<String>,
    /// `driver-opt=`, repeatable.
    ///
    /// `Option` because Go's `map[string]string` distinguishes an absent
    /// driver option from an empty one, and `--network` without `driver-opt=`
    /// must keep it absent.
    pub driver_opts: Option<BTreeMap<String, String>>,
    /// Never set: the CSV notation has no spelling for a link. Upstream
    /// carries the field and a `TODO` about it.
    pub links: Vec<String>,
    /// `ip=`.
    pub ipv4_address: Option<std::net::IpAddr>,
    /// `ip6=`.
    pub ipv6_address: Option<std::net::IpAddr>,
    /// `link-local-ip=`, repeatable.
    pub link_local_ips: Vec<std::net::IpAddr>,
    /// `mac-address=`.
    pub mac_address: String,
    /// `gw-priority=`.
    pub gw_priority: i64,
}

/// The option keys of the advanced `--network` syntax.
const NETWORK_OPT_NAME: &str = "name";
const NETWORK_OPT_ALIAS: &str = "alias";
const NETWORK_OPT_IPV4_ADDRESS: &str = "ip";
const NETWORK_OPT_IPV6_ADDRESS: &str = "ip6";
const NETWORK_OPT_MAC_ADDRESS: &str = "mac-address";
const NETWORK_OPT_LINK_LOCAL_IP: &str = "link-local-ip";
const NETWORK_OPT_DRIVER: &str = "driver-opt";
const NETWORK_OPT_GW_PRIORITY: &str = "gw-priority";

/// `regexp.MatchString(`\w+=\w+(,\w+=\w+)*`, value)`: does the value contain a
/// `k=v` pair anywhere?
///
/// The pattern is unanchored and its `(,\w+=\w+)*` tail is optional, so a match
/// exists as soon as `\w+=\w+` occurs *somewhere* — which is why
/// `gw-priority=invalid-integer` is advanced syntax (the `priority=invalid`
/// inside it matches) while `name=` is not: the second `\w+` needs a word
/// character, and the string ends.
///
/// `(?-u)` matters: Go's `\w` is ASCII-only, while the `regex` crate's is
/// Unicode-aware by default, so `--network 'name=café'` would otherwise be
/// advanced syntax here and legacy there.
fn is_long_network_syntax(value: &str) -> bool {
    static LONG_SYNTAX: OnceLock<Regex> = OnceLock::new();
    LONG_SYNTAX
        .get_or_init(|| {
            Regex::new(r"(?-u)\w+=\w+(,\w+=\w+)*").expect("the network option grammar")
        })
        .is_match(value)
}

/// `parseDriverOpt`: the `k=v` *inside* a `driver-opt=` value.
fn parse_driver_opt(driver_opt: &str) -> Result<(String, String), String> {
    // The whole option was already lowercased with the field around it, which
    // is why the sysctl example in the test table comes back with `IFNAME`
    // turned into `ifname`.
    let Some((key, value)) = driver_opt.split_once('=') else {
        return Err("invalid key value pair format in driver options".to_string());
    };
    if key.is_empty() {
        return Err("invalid key value pair format in driver options".to_string());
    }
    Ok((key.trim().to_string(), value.trim().to_string()))
}

/// `opts.NetworkOpt`: the repeatable `--network` flag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkOpt {
    options: Vec<NetworkAttachmentOpts>,
}

impl NetworkOpt {
    /// `Type()`.
    pub fn type_name(&self) -> &'static str {
        "network"
    }

    /// `Value()`: the attachments parsed so far, in the order they were given.
    pub fn value(&self) -> &[NetworkAttachmentOpts] {
        &self.options
    }

    /// `NetworkMode()`: the target of the **first** attachment, or `"default"`.
    ///
    /// Not the last, and not the whole list: `--network` is documented as
    /// repeatable and every value is parsed and validated, but only the first
    /// one ever reaches a `HostConfig`. The literal `"default"` for an unset
    /// flag is the daemon's own name for "the default bridge network".
    pub fn network_mode(&self) -> &str {
        self.options
            .first()
            .map_or("default", |option| option.target.as_str())
    }

    /// `Set`: one `--network` value, appended on success.
    ///
    /// The two syntaxes are chosen by `is_long_network_syntax`, not by the
    /// presence of a `name=`, which is what makes `docknet1` a target and
    /// `name=` — with nothing after the `=` — a target as well.
    pub fn set(&mut self, value: &str) -> Result<(), String> {
        let mut net_opt = NetworkAttachmentOpts::default();
        if is_long_network_syntax(value) {
            let fields = csv::read_record(value)?;
            // Set, not left nil: the advanced syntax always has an alias list.
            net_opt.aliases = Vec::new();
            for field in fields {
                // The *whole* field is lowercased, so both the key and the
                // value of a `driver-opt=` lose their case.
                // `TODO(thaJeztah): these options should not be case-insensitive.`
                let lowered = field.to_lowercase();
                let (key, val, ok) = match lowered.split_once('=') {
                    Some((key, val)) => (key, val, true),
                    None => (lowered.as_str(), "", false),
                };
                // The emptiness check is on the *untrimmed* key, so `" =x"`
                // slips past it and is rejected as an unknown key instead.
                if !ok || key.is_empty() {
                    return Err(format!("invalid field {field}"));
                }
                let key = key.trim();
                let val = val.trim();
                match key {
                    NETWORK_OPT_NAME => net_opt.target = val.to_string(),
                    NETWORK_OPT_ALIAS => net_opt.aliases.push(val.to_string()),
                    NETWORK_OPT_IPV4_ADDRESS => net_opt.ipv4_address = Some(go_parse_addr(val)?),
                    NETWORK_OPT_IPV6_ADDRESS => net_opt.ipv6_address = Some(go_parse_addr(val)?),
                    NETWORK_OPT_MAC_ADDRESS => net_opt.mac_address = val.to_string(),
                    NETWORK_OPT_LINK_LOCAL_IP => net_opt.link_local_ips.push(go_parse_addr(val)?),
                    NETWORK_OPT_DRIVER => {
                        let (driver_key, driver_value) = parse_driver_opt(val)?;
                        net_opt
                            .driver_opts
                            .get_or_insert_with(BTreeMap::new)
                            .insert(driver_key, driver_value);
                    }
                    NETWORK_OPT_GW_PRIORITY => {
                        net_opt.gw_priority = go_atoi(val)
                            .map_err(|reason| format!("invalid gw-priority ({val}): {reason}"))?;
                    }
                    other => return Err(format!("invalid field key {other}")),
                }
            }
            if net_opt.target.is_empty() {
                return Err("network name/id is not specified".to_string());
            }
        } else {
            net_opt.target = value.to_string();
        }
        self.options.push(net_opt);
        Ok(())
    }
}

impl fmt::Display for NetworkOpt {
    /// `String()`: unconditionally `""`, for every value of the flag. Upstream
    /// leaves the method body empty; the flag's real content is
    /// [`NetworkOpt::network_mode`].
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// GpuOpts
// ---------------------------------------------------------------------------

/// `opts.parseCount`: `"all"` or an integer.
fn parse_gpu_count(value: &str) -> Result<i64, String> {
    if value == "all" {
        return Ok(-1);
    }
    go_atoi(value).map_err(|reason| {
        format!("invalid count ({value}): value must be either \"all\" or an integer: {reason}")
    })
}

/// `opts.GpuOpts`: the repeatable `--gpus` flag, producing one
/// [`DeviceRequest`] per value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GpuOpts {
    values: Vec<DeviceRequest>,
}

impl GpuOpts {
    /// `Type()`.
    pub fn type_name(&self) -> &'static str {
        "gpu-request"
    }

    /// `Value()`: the requests parsed so far, in the order they were given.
    pub fn value(&self) -> &[DeviceRequest] {
        &self.values
    }

    /// `Set`: one `--gpus` value, appended on success.
    ///
    /// A field with no `=` is a *count*, and a duplicate key is an error even
    /// though the map that detects it is a `map[string]struct{}` upstream. Note
    /// what is not here: no whitespace check and no empty-value check, so
    /// `count= 2` and `driver=` are rejected by whatever they fail in rather
    /// than by a rule of their own, and `--gpus ""` is the CSV reader's `EOF`.
    pub fn set(&mut self, value: &str) -> Result<(), String> {
        let fields = csv::read_record(value)?;
        let mut req = DeviceRequest::default();
        let mut seen: BTreeSet<&str> = BTreeSet::new();

        for field in &fields {
            let (key, val, with_value) = match field.split_once('=') {
                Some((key, val)) => (key, val, true),
                None => (field.as_str(), "", false),
            };
            if !seen.insert(key) {
                return Err(format!("gpu request key '{key}' can be specified only once"));
            }

            if !with_value {
                // The bare key is the count, so it is marked seen as "count" —
                // which is why `--gpus 1,count=1` is a duplicate and not a
                // silent overwrite.
                seen.insert("count");
                req.count = parse_gpu_count(key)?;
                continue;
            }

            match key {
                "driver" => req.driver = val.to_string(),
                "count" => req.count = parse_gpu_count(val)?,
                "device" => req.device_ids = val.split(',').map(str::to_string).collect(),
                "capabilities" => {
                    // "gpu" is always appended, so `--gpus capabilities=compute`
                    // asks for `["compute", "gpu"]` and never for compute alone.
                    let mut capabilities: Vec<String> =
                        val.split(',').map(str::to_string).collect();
                    capabilities.push("gpu".to_string());
                    req.capabilities = vec![capabilities];
                }
                "options" => {
                    // A second CSV pass, so `options="a=1,b=2"` is one option
                    // and not two.
                    let option_fields = csv::read_record(val)
                        .map_err(|err| format!("failed to read gpu options: {err}"))?;
                    req.options =
                        super::docker_opts::convert_kv_strings_to_map(&option_fields);
                }
                other => return Err(format!("unexpected key '{other}' in '{field}'")),
            }
        }

        // Naming devices pins the count, so a bare `--gpus device=0` asks for
        // device `0` and not for "one of them". `device=` yields a one-element
        // list holding the empty string, which is also a pinned count.
        if !seen.contains("count") && req.device_ids.is_empty() {
            req.count = 1;
        }
        if req.capabilities.is_empty() {
            req.capabilities = vec![vec!["gpu".to_string()]];
        }
        self.values.push(req);
        Ok(())
    }
}

impl fmt::Display for GpuOpts {
    /// `String()`: each request as Go's `%v` prints a `DeviceRequest` —
    /// `{driver count [ids] [[capabilities]] map[option:value]}` — `", "`-joined.
    ///
    /// A nil Go slice prints `[]` and a nil map prints `map[]`, so an unset
    /// request reads `{ 1 [] [[gpu]] map[]}`. The map is sorted because Go
    /// sorts map keys when printing them.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reprs: Vec<String> = self.values.iter().map(go_value_v).collect();
        f.write_str(&reprs.join(", "))
    }
}

/// Go's `%v` for a `container.DeviceRequest`, field by field.
fn go_value_v(request: &DeviceRequest) -> String {
    let ids = request.device_ids.join(" ");
    let capabilities = request
        .capabilities
        .iter()
        .map(|set| format!("[{}]", set.join(" ")))
        .collect::<Vec<String>>()
        .join(" ");
    let options = request
        .options
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<String>>()
        .join(" ");
    format!(
        "{{{} {} [{}] [{}] map[{}]}}",
        request.driver, request.count, ids, capabilities, options
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    /// A `map[string]string` from pairs, for the expectations that name one.
    fn map_of(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn addr(value: &str) -> IpAddr {
        value.parse().expect("a literal address")
    }

    /// The `--mount` value the boolean tables append their option to.
    fn bind_mount(option: &str) -> MountOpt {
        let value = if option.is_empty() {
            "type=bind,target=/foo,source=/foo".to_string()
        } else {
            format!("type=bind,target=/foo,source=/foo,{option}")
        };
        let mut m = MountOpt::default();
        m.set(&value).expect("a valid mount");
        m
    }

    fn volume_mount(option: &str) -> MountOpt {
        let value = if option.is_empty() {
            "type=volume,target=/foo,source=foo".to_string()
        } else {
            format!("type=volume,target=/foo,source=foo,{option}")
        };
        let mut m = MountOpt::default();
        m.set(&value).expect("a valid mount");
        m
    }

    // encoding/csv: the cases the three flag tables above do not reach. The
    // values are the ones a shell can actually hand to --mount.
    #[test]
    fn a_quoted_field_keeps_its_commas() {
        assert_eq!(
            csv::read_record(r#"a,"b,c",d"#).expect("a record"),
            vec!["a", "b,c", "d"],
        );
        // The end of the value is not the end of the reader: a value with no
        // trailing newline is still a record, not an EOF.
        assert_eq!(
            csv::read_record("a,b").expect("a record"),
            vec!["a", "b"],
        );
        assert_eq!(csv::read_record(""), Err("EOF".to_string()));
    }

    #[test]
    fn a_quoted_field_may_span_lines() {
        assert_eq!(
            csv::read_record("\"a\nb\",c").expect("a record"),
            vec!["a\nb", "c"],
        );
    }

    #[test]
    fn a_doubled_quote_is_one_quote() {
        assert_eq!(
            csv::read_record(r#""sou""rce""#).expect("a record"),
            vec![r#"sou"rce"#],
        );
    }

    #[test]
    fn an_unterminated_quoted_field_is_an_error() {
        // The column is the end of the copied data, not the start of the
        // field, which is why it is 3 for `"a` and 5 for `"abc`.
        assert_eq!(
            csv::read_record("\"a"),
            Err("parse error on line 1, column 3: extraneous or missing \" in quoted-field"
                .to_string()),
        );
        assert_eq!(
            csv::read_record("\"abc"),
            Err("parse error on line 1, column 5: extraneous or missing \" in quoted-field"
                .to_string()),
        );
    }

    #[test]
    fn a_quote_inside_an_unquoted_field_is_an_error() {
        assert_eq!(
            csv::read_record("a\"b"),
            Err("parse error on line 1, column 2: bare \" in non-quoted-field".to_string()),
        );
        // Which is what a user gets for `source="a,b"`: the field has to be
        // quoted whole, or the value is a bare-quote error.
        let mut m = MountOpt::default();
        assert_eq!(
            m.set(r#"type=bind,source="a,b",target=/c"#),
            Err("parse error on line 1, column 18: bare \" in non-quoted-field".to_string()),
        );
        m.set(r#"type=bind,"source=a,b",target=/c"#)
            .expect("a quoted field");
        assert_eq!(m.value()[0].source, "a,b");
    }

    // mount_test.go: TestMountOptString
    #[test]
    fn mount_opt_string_joins_three_fields_per_mount() {
        let m = MountOpt {
            values: vec![
                mount::Mount {
                    mount_type: mount::MountType(mount::TYPE_BIND.to_string()),
                    source: "/home/path".to_string(),
                    target: "/target".to_string(),
                    ..Default::default()
                },
                mount::Mount {
                    mount_type: mount::MountType(mount::TYPE_VOLUME.to_string()),
                    source: "foo".to_string(),
                    target: "/target/foo".to_string(),
                    ..Default::default()
                },
            ],
        };
        assert_eq!(
            m.to_string(),
            "bind /home/path /target, volume foo /target/foo"
        );
        assert_eq!(MountOpt::default().to_string(), "");
    }

    // mount_test.go: TestMountRelative
    #[test]
    fn a_relative_bind_source_becomes_absolute() {
        for path in [".", "./", "..", "../"] {
            let mut m = MountOpt::default();
            m.set(&format!("type=bind,source={path},target=/target"))
                .expect("a valid mount");
            assert_eq!(m.value().len(), 1, "{path:?}");
            // `filepath.Abs` is `Clean(Join(wd, path))`, spelled out here so the
            // expectation does not go through the function under test.
            let wd = std::env::current_dir().expect("a working directory");
            let expected = super::super::docker_opts::slash_clean(&format!(
                "{}/{}",
                wd.to_string_lossy(),
                path
            ));
            assert_eq!(
                m.value()[0],
                mount::Mount {
                    mount_type: mount::MountType(mount::TYPE_BIND.to_string()),
                    source: expected,
                    target: "/target".to_string(),
                    ..Default::default()
                },
                "source={path:?}",
            );
        }
    }

    // mount_test.go: TestMountOptSourceTargetAliases
    #[test]
    fn the_source_and_target_aliases_agree() {
        for value in [
            "type=bind,src=/source,dst=/target",
            "type=bind,source=/source,target=/target",
            "type=bind,source=/source,destination=/target",
        ] {
            let mut m = MountOpt::default();
            m.set(value).expect("a valid mount");
            assert_eq!(m.value().len(), 1, "{value}");
            assert_eq!(
                m.value()[0],
                mount::Mount {
                    mount_type: mount::MountType(mount::TYPE_BIND.to_string()),
                    source: "/source".to_string(),
                    target: "/target".to_string(),
                    ..Default::default()
                },
                "{value}",
            );
        }
    }

    // mount_test.go: TestMountOptDefaultType
    #[test]
    fn a_mount_without_a_type_is_a_volume() {
        let mut m = MountOpt::default();
        m.set("target=/target,source=/foo").expect("a valid mount");
        assert_eq!(m.value()[0].mount_type.0, mount::TYPE_VOLUME);
    }

    // mount_test.go: TestMountOptErrors
    #[test]
    fn mount_opt_rejects_bad_values_with_these_messages() {
        for (doc, value, expected) in [
            ("empty value", "", "value is empty"),
            (
                "invalid key=value",
                "type=volume,target=/foo,bogus=foo",
                "unknown option 'bogus' in 'bogus=foo'",
            ),
            (
                "invalid key with leading whitespace",
                "type=volume, src=/foo,target=/foo",
                "invalid option 'src' in ' src=/foo': option should not have whitespace",
            ),
            (
                "invalid key with trailing whitespace",
                "type=volume,src =/foo,target=/foo",
                "invalid option 'src' in 'src =/foo': option should not have whitespace",
            ),
            (
                "invalid value is empty",
                "type=volume,src=,target=/foo",
                "invalid value for 'src': value is empty",
            ),
            (
                "invalid value with leading whitespace",
                "type=volume,src= /foo,target=/foo",
                "invalid value for 'src' in 'src= /foo': value should not have whitespace",
            ),
            (
                "invalid value with trailing whitespace",
                "type=volume,src=/foo ,target=/foo",
                "invalid value for 'src' in 'src=/foo ': value should not have whitespace",
            ),
            (
                "missing value",
                "type=volume,target=/foo,bogus",
                "invalid field 'bogus' must be a key=value pair",
            ),
            (
                "invalid tmpfs-size",
                "type=tmpfs,target=/foo,tmpfs-size=foo",
                "invalid value for tmpfs-size: foo",
            ),
            (
                "invalid tmpfs-mode",
                "type=tmpfs,target=/foo,tmpfs-mode=foo",
                "invalid value for tmpfs-mode: foo",
            ),
            (
                "mixed bind and volume",
                "type=volume,target=/foo,source=/foo,bind-propagation=rprivate",
                "cannot mix 'bind-*' options with mount type 'volume'",
            ),
            (
                "mixed volume and bind",
                "type=bind,target=/foo,source=/foo,volume-nocopy=true",
                "cannot mix 'volume-*' options with mount type 'bind'",
            ),
        ] {
            assert_eq!(
                MountOpt::default().set(value).expect_err(doc),
                expected,
                "{doc}"
            );
        }
    }

    // mount_test.go: TestMountOptReadOnly
    #[test]
    fn mount_opt_read_only_takes_its_spellings() {
        for (value, expected) in [
            ("readonly", true),
            ("readonly=1", true),
            ("readonly=true", true),
            ("readonly=0", false),
            ("readonly=false", false),
            ("ro", true),
            ("ro=1", true),
            ("ro=true", true),
            ("ro=0", false),
            ("ro=false", false),
        ] {
            let m = bind_mount(value);
            assert_eq!(m.value()[0].read_only, expected, "{value}");
        }
        assert!(!bind_mount("").value()[0].read_only, "not set");

        for (value, expected) in [
            (
                "readonly=",
                "invalid value for 'readonly': value is empty",
            ),
            (
                "readonly= true",
                "invalid value for 'readonly' in 'readonly= true': value should not have whitespace",
            ),
            (
                "readonly=no",
                "invalid value for 'readonly': invalid boolean value (\"no\"): must be one of \"true\", \"1\", \"false\", or \"0\" (default \"true\")",
            ),
        ] {
            let mut m = MountOpt::default();
            assert_eq!(
                m.set(&format!("type=bind,target=/foo,source=/foo,{value}"))
                    .expect_err(value),
                expected,
                "{value}"
            );
        }
    }

    // mount_test.go: TestMountOptVolumeNoCopy
    #[test]
    fn mount_opt_volume_nocopy_takes_its_spellings() {
        for (value, expected) in [
            ("volume-nocopy", true),
            ("volume-nocopy=1", true),
            ("volume-nocopy=true", true),
            ("volume-nocopy=0", false),
            ("volume-nocopy=false", false),
        ] {
            let m = volume_mount(value);
            assert_eq!(
                m.value()[0].volume_options.as_ref().expect(value).no_copy,
                expected,
                "{value}"
            );
        }
        // A mount with no `volume-*` option at all has *no* volume options,
        // which is the nil-versus-empty distinction an enum would lose.
        assert_eq!(volume_mount("").value()[0].volume_options, None);

        for (value, expected) in [
            (
                "volume-nocopy=",
                "invalid value for 'volume-nocopy': value is empty",
            ),
            (
                "volume-nocopy= true",
                "invalid value for 'volume-nocopy' in 'volume-nocopy= true': value should not have whitespace",
            ),
            (
                "volume-nocopy=no",
                "invalid value for 'volume-nocopy': invalid boolean value (\"no\"): must be one of \"true\", \"1\", \"false\", or \"0\" (default \"true\")",
            ),
        ] {
            let mut m = MountOpt::default();
            assert_eq!(
                m.set(&format!("type=volume,target=/foo,source=foo,{value}"))
                    .expect_err(value),
                expected,
                "{value}"
            );
        }
    }

    // mount_test.go: TestMountOptVolumeOptions
    #[test]
    fn mount_opt_builds_volume_labels_and_driver_options() {
        let volume = |options: mount::VolumeOptions| mount::Mount {
            mount_type: mount::MountType(mount::TYPE_VOLUME.to_string()),
            target: "/foo".to_string(),
            volume_options: Some(options),
            ..Default::default()
        };
        for (doc, value, expected) in [
            (
                "volume-label single",
                "type=volume,target=/foo,volume-label=foo=foo-value",
                volume(mount::VolumeOptions {
                    labels: Some(map_of(&[("foo", "foo-value")])),
                    ..Default::default()
                }),
            ),
            (
                "volume-label multiple",
                "type=volume,target=/foo,volume-label=foo=foo-value,volume-label=bar=bar-value",
                volume(mount::VolumeOptions {
                    labels: Some(map_of(&[("foo", "foo-value"), ("bar", "bar-value")])),
                    ..Default::default()
                }),
            ),
            (
                // The source reads as if `volume-label=bar` should be an empty
                // value error, and it is not: the *field* has a value, and
                // `setValueOnMap` then reads `bar` as a key with no `=`.
                // Probed against go1.26.2, which accepts it.
                "volume-label empty values",
                "type=volume,target=/foo,volume-label=foo=,volume-label=bar",
                volume(mount::VolumeOptions {
                    labels: Some(map_of(&[("foo", ""), ("bar", "")])),
                    ..Default::default()
                }),
            ),
            (
                // An empty *key* drops the pair, so the options are created and
                // then left empty. Upstream: "this should probably be an error
                // instead".
                "volume-label empty key",
                "type=volume,target=/foo,volume-label==foo-value",
                volume(mount::VolumeOptions::default()),
            ),
            (
                "volume-driver",
                "type=volume,target=/foo,volume-driver=my-driver",
                volume(mount::VolumeOptions {
                    driver_config: Some(mount::Driver {
                        name: "my-driver".to_string(),
                        options: None,
                    }),
                    ..Default::default()
                }),
            ),
            (
                "volume-opt single",
                "type=volume,target=/foo,volume-opt=foo=foo-value",
                volume(mount::VolumeOptions {
                    driver_config: Some(mount::Driver {
                        name: String::new(),
                        options: Some(map_of(&[("foo", "foo-value")])),
                    }),
                    ..Default::default()
                }),
            ),
            (
                "volume-opt multiple",
                "type=volume,target=/foo,volume-opt=foo=foo-value,volume-opt=bar=bar-value",
                volume(mount::VolumeOptions {
                    driver_config: Some(mount::Driver {
                        name: String::new(),
                        options: Some(map_of(&[("foo", "foo-value"), ("bar", "bar-value")])),
                    }),
                    ..Default::default()
                }),
            ),
            (
                "volume-opt empty values",
                "type=volume,target=/foo,volume-opt=foo=,volume-opt=bar",
                volume(mount::VolumeOptions {
                    driver_config: Some(mount::Driver {
                        name: String::new(),
                        options: Some(map_of(&[("foo", ""), ("bar", "")])),
                    }),
                    ..Default::default()
                }),
            ),
            (
                "volume-opt empty key",
                "type=volume,target=/foo,volume-opt==foo-value",
                volume(mount::VolumeOptions {
                    driver_config: Some(mount::Driver::default()),
                    ..Default::default()
                }),
            ),
            (
                "volume-label and volume-opt",
                "type=volume,volume-driver=my-driver,target=/foo,volume-label=foo=foo-value,volume-label=empty=,volume-opt=foo=foo-value,volume-opt=empty=",
                volume(mount::VolumeOptions {
                    labels: Some(map_of(&[("foo", "foo-value"), ("empty", "")])),
                    driver_config: Some(mount::Driver {
                        name: "my-driver".to_string(),
                        options: Some(map_of(&[("foo", "foo-value"), ("empty", "")])),
                    }),
                    ..Default::default()
                }),
            ),
        ] {
            let mut m = MountOpt::default();
            m.set(value).unwrap_or_else(|err| panic!("{doc}: {err}"));
            assert_eq!(m.value()[0], expected, "{doc}");
        }
    }

    // mount_test.go: TestMountOptSetImageNoError
    #[test]
    fn mount_opt_accepts_an_image_mount() {
        let mut m = MountOpt::default();
        m.set("type=image,source=foo,target=/target,image-subpath=/bar")
            .expect("a valid mount");
        assert_eq!(m.value().len(), 1);
        assert_eq!(
            m.value()[0],
            mount::Mount {
                mount_type: mount::MountType(mount::TYPE_IMAGE.to_string()),
                source: "foo".to_string(),
                target: "/target".to_string(),
                image_options: Some(mount::ImageOptions {
                    subpath: "/bar".to_string(),
                }),
                ..Default::default()
            }
        );
    }

    // mount_test.go: TestMountOptSetTmpfsNoError
    #[test]
    fn mount_opt_reads_tmpfs_size_in_binary_and_mode_as_octal() {
        for value in [
            "type=tmpfs,target=/target,tmpfs-size=1m,tmpfs-mode=0700",
            "type=tmpfs,target=/target,tmpfs-size=1MB,tmpfs-mode=700",
        ] {
            let mut m = MountOpt::default();
            m.set(value).expect("a valid mount");
            assert_eq!(m.value().len(), 1, "{value}");
            assert_eq!(
                m.value()[0],
                mount::Mount {
                    mount_type: mount::MountType(mount::TYPE_TMPFS.to_string()),
                    target: "/target".to_string(),
                    // 1024 * 1024, not 1000 * 1000.
                    tmpfs_options: Some(mount::TmpfsOptions {
                        size_bytes: 1024 * 1024,
                        mode: 0o700,
                        options: Vec::new(),
                    }),
                    ..Default::default()
                },
                "{value}"
            );
        }
    }

    // mount_test.go: TestMountOptSetBindCreateSrc
    #[test]
    fn mount_opt_bind_create_src_takes_its_spellings() {
        for (value, expected) in [
            ("bind-create-src", true),
            ("bind-create-src=1", true),
            ("bind-create-src=true", true),
            ("bind-create-src=0", false),
            ("bind-create-src=false", false),
        ] {
            let m = bind_mount(value);
            assert_eq!(
                m.value()[0]
                    .bind_options
                    .as_ref()
                    .expect(value)
                    .create_mountpoint,
                expected,
                "{value}"
            );
        }
        assert_eq!(bind_mount("").value()[0].bind_options, None, "not set");

        for (value, expected) in [
            (
                "bind-create-src=",
                "invalid value for 'bind-create-src': value is empty",
            ),
            (
                "bind-create-src= true",
                "invalid value for 'bind-create-src' in 'bind-create-src= true': value should not have whitespace",
            ),
            (
                "bind-create-src=no",
                "invalid value for 'bind-create-src': invalid boolean value (\"no\"): must be one of \"true\", \"1\", \"false\", or \"0\" (default \"true\")",
            ),
        ] {
            let mut m = MountOpt::default();
            assert_eq!(
                m.set(&format!("type=bind,target=/foo,source=/foo,{value}"))
                    .expect_err(value),
                expected,
                "{value}"
            );
        }
    }

    // mount_test.go: TestMountOptSetBindRecursive
    #[test]
    fn mount_opt_bind_recursive_keeps_its_four_reachability_rules() {
        let bind = || mount::Mount {
            mount_type: mount::MountType(mount::TYPE_BIND.to_string()),
            source: "/foo".to_string(),
            target: "/bar".to_string(),
            ..Default::default()
        };

        // "enabled" is a NOP: no `BindOptions` is allocated at all.
        let mut m = MountOpt::default();
        m.set("type=bind,source=/foo,target=/bar,bind-recursive=enabled")
            .expect("a valid mount");
        assert_eq!(m.value(), &[bind()]);

        let mut m = MountOpt::default();
        m.set("type=bind,source=/foo,target=/bar,bind-recursive=disabled")
            .expect("a valid mount");
        assert_eq!(
            m.value(),
            &[mount::Mount {
                bind_options: Some(mount::BindOptions {
                    non_recursive: true,
                    ..Default::default()
                }),
                ..bind()
            }]
        );

        let mut m = MountOpt::default();
        assert_eq!(
            m.set("type=bind,source=/foo,target=/bar,bind-recursive=writable"),
            Err("option 'bind-recursive=writable' requires 'readonly' to be specified in conjunction"
                .to_string())
        );
        m.set("type=bind,source=/foo,target=/bar,bind-recursive=writable,readonly")
            .expect("a valid mount");
        assert_eq!(
            m.value(),
            &[mount::Mount {
                read_only: true,
                bind_options: Some(mount::BindOptions {
                    read_only_non_recursive: true,
                    ..Default::default()
                }),
                ..bind()
            }]
        );

        let mut m = MountOpt::default();
        assert_eq!(
            m.set("type=bind,source=/foo,target=/bar,bind-recursive=readonly"),
            Err("option 'bind-recursive=readonly' requires 'readonly' to be specified in conjunction"
                .to_string())
        );
        assert_eq!(
            m.set("type=bind,source=/foo,target=/bar,bind-recursive=readonly,readonly"),
            Err("option 'bind-recursive=readonly' requires 'bind-propagation=rprivate' to be specified in conjunction"
                .to_string())
        );
        m.set(
            "type=bind,source=/foo,target=/bar,bind-recursive=readonly,readonly,bind-propagation=rprivate",
        )
        .expect("a valid mount");
        assert_eq!(
            m.value(),
            &[mount::Mount {
                read_only: true,
                bind_options: Some(mount::BindOptions {
                    read_only_force_recursive: true,
                    propagation: mount::Propagation("rprivate".to_string()),
                    ..Default::default()
                }),
                ..bind()
            }]
        );
    }

    // mount_test.go: has no table for these two, but the module doc claims the
    // behaviours, so they are asserted rather than asserted-in-prose.
    #[test]
    fn a_deprecated_or_typed_option_fails_the_way_upstream_does() {
        assert_eq!(
            MountOpt::default().set("type=bind,source=/foo,target=/bar,bind-nonrecursive"),
            Err("bind-nonrecursive is deprecated, use bind-recursive=disabled instead".to_string())
        );
        // A type the client has never heard of is the daemon's problem.
        let mut m = MountOpt::default();
        m.set("type=bogus,source=/foo,target=/bar")
            .expect("an unknown type is not a client-side error");
        assert_eq!(m.value()[0].mount_type.0, "bogus");
    }

    #[test]
    fn mount_opt_accumulates_repeated_flags() {
        let mut m = MountOpt::default();
        m.set("type=bind,source=/foo,target=/bar").expect("valid");
        m.set("type=volume,target=/baz").expect("valid");
        assert_eq!(m.value().len(), 2);
        // A rejected value appends nothing and keeps what came before.
        assert!(m.set("type=volume,bogus").is_err());
        assert_eq!(m.value().len(), 2);
        assert_eq!(m.type_name(), "mount");
    }

    // network_test.go: TestNetworkOptLegacySyntax
    #[test]
    fn network_opt_legacy_syntax_is_a_bare_target() {
        let mut n = NetworkOpt::default();
        n.set("docknet1").expect("a valid network");
        assert_eq!(
            n.value(),
            &[NetworkAttachmentOpts {
                target: "docknet1".to_string(),
                ..Default::default()
            }]
        );
    }

    // network_test.go: TestNetworkOptAdvancedSyntax
    #[test]
    fn network_opt_advanced_syntax_parses_its_options() {
        for (value, expected) in [
            (
                "name=docknet1,alias=web,driver-opt=field1=value1",
                NetworkAttachmentOpts {
                    target: "docknet1".to_string(),
                    aliases: vec!["web".to_string()],
                    driver_opts: Some(map_of(&[("field1", "value1")])),
                    ..Default::default()
                },
            ),
            (
                "name=docknet1,alias=web1,alias=web2,driver-opt=field1=value1,driver-opt=field2=value2",
                NetworkAttachmentOpts {
                    target: "docknet1".to_string(),
                    aliases: vec!["web1".to_string(), "web2".to_string()],
                    driver_opts: Some(map_of(&[("field1", "value1"), ("field2", "value2")])),
                    ..Default::default()
                },
            ),
            (
                "name=docknet1,ip=172.20.88.22,ip6=2001:db8::8822",
                NetworkAttachmentOpts {
                    target: "docknet1".to_string(),
                    ipv4_address: Some(addr("172.20.88.22")),
                    ipv6_address: Some(addr("2001:db8::8822")),
                    ..Default::default()
                },
            ),
            (
                "name=docknet1",
                NetworkAttachmentOpts {
                    target: "docknet1".to_string(),
                    ..Default::default()
                },
            ),
            (
                "name=docknet1,mac-address=52:0f:f3:dc:50:10",
                NetworkAttachmentOpts {
                    target: "docknet1".to_string(),
                    mac_address: "52:0f:f3:dc:50:10".to_string(),
                    ..Default::default()
                },
            ),
            (
                "name=docknet1,link-local-ip=169.254.169.254,link-local-ip=169.254.10.10",
                NetworkAttachmentOpts {
                    target: "docknet1".to_string(),
                    link_local_ips: vec![addr("169.254.169.254"), addr("169.254.10.10")],
                    ..Default::default()
                },
            ),
            (
                // The CLI lowercases the whole field, so `IFNAME` arrives at the
                // API as `ifname` — upstream notes that it probably shouldn't.
                r#"name=docknet1,"driver-opt=com.docker.network.endpoint.sysctls=net.ipv6.conf.IFNAME.accept_ra=2,net.ipv6.conf.IFNAME.forwarding=1""#,
                NetworkAttachmentOpts {
                    target: "docknet1".to_string(),
                    driver_opts: Some(map_of(&[(
                        "com.docker.network.endpoint.sysctls",
                        "net.ipv6.conf.ifname.accept_ra=2,net.ipv6.conf.ifname.forwarding=1",
                    )])),
                    ..Default::default()
                },
            ),
            (
                "name=docknet1,gw-priority=10",
                NetworkAttachmentOpts {
                    target: "docknet1".to_string(),
                    gw_priority: 10,
                    ..Default::default()
                },
            ),
        ] {
            let mut n = NetworkOpt::default();
            n.set(value).unwrap_or_else(|err| panic!("{value}: {err}"));
            assert_eq!(n.value(), &[expected], "{value}");
        }
    }

    // network_test.go: TestNetworkOptAdvancedSyntaxInvalid
    #[test]
    fn network_opt_advanced_syntax_rejects_these_values() {
        for (value, expected) in [
            ("invalidField=docknet1", "invalid field key invalidfield"),
            ("network=docknet1,invalid=web", "invalid field key network"),
            (
                "driver-opt=field1=value1,driver-opt=field2=value2",
                "network name/id is not specified",
            ),
            (
                "gw-priority=invalid-integer",
                "invalid gw-priority (invalid-integer): invalid syntax",
            ),
        ] {
            assert_eq!(
                NetworkOpt::default().set(value).expect_err(value),
                expected,
                "{value}"
            );
        }
    }

    // network_test.go: TestNetworkOptStringNetOptString and
    // TestNetworkOptTypeNetOptType
    #[test]
    fn network_opt_string_is_empty_and_its_type_is_network() {
        let n = NetworkOpt::default();
        assert_eq!(n.to_string(), "");
        assert_eq!(n.type_name(), "network");
    }

    #[test]
    fn network_mode_reads_the_first_target_or_says_default() {
        // The module doc's claim: repeatable, but only the first is used.
        let mut n = NetworkOpt::default();
        assert_eq!(n.network_mode(), "default");
        n.set("docknet1").expect("a valid network");
        n.set("docknet2").expect("a valid network");
        assert_eq!(n.network_mode(), "docknet1");
        assert_eq!(n.value().len(), 2, "both were parsed and kept");

        // Legacy syntax is chosen by the absence of *any* `k=v` in the value,
        // so `name=` is a network called `name=`, not an option with no value.
        let mut n = NetworkOpt::default();
        n.set("name=").expect("a valid network");
        assert_eq!(n.network_mode(), "name=");

        // Go's `\w` is ASCII-only, so a value with a non-ASCII word character
        // where the key would be is not a pair — and `\w+` is allowed to stop
        // short, so `name=café` is advanced with the target `café`. Both probed
        // against go1.26.2.
        let mut n = NetworkOpt::default();
        n.set("café=x").expect("a valid network");
        assert_eq!(n.value()[0].target, "café=x");
        let mut n = NetworkOpt::default();
        n.set("name=café").expect("a valid network");
        assert_eq!(n.value()[0].target, "café");
    }

    #[test]
    fn network_opt_reports_an_unparsable_address() {
        assert_eq!(
            NetworkOpt::default().set("name=docknet1,ip=nope"),
            Err("ParseAddr(\"nope\"): unable to parse IP".to_string())
        );
        // A gw-priority that does not fit an int is out of range, not a syntax
        // error, and the two are not interchangeable in the message.
        assert_eq!(
            NetworkOpt::default().set("name=docknet1,gw-priority=99999999999999999999"),
            Err("invalid gw-priority (99999999999999999999): value out of range".to_string())
        );
    }

    // gpus_test.go: TestGpusOptAll
    #[test]
    fn gpus_opt_all_asks_for_every_device() {
        for value in ["all", "-1", "count=all", "count=-1"] {
            let mut g = GpuOpts::default();
            g.set(value).unwrap_or_else(|err| panic!("{value}: {err}"));
            assert_eq!(g.value().len(), 1, "{value}");
            assert_eq!(
                g.value()[0],
                DeviceRequest {
                    count: -1,
                    capabilities: vec![vec!["gpu".to_string()]],
                    ..Default::default()
                },
                "{value}"
            );
        }
    }

    // gpus_test.go: TestGpusOptInvalidCount
    #[test]
    fn gpus_opt_rejects_a_count_that_is_neither_all_nor_an_integer() {
        assert_eq!(
            GpuOpts::default().set("count=invalid-integer"),
            Err("invalid count (invalid-integer): value must be either \"all\" or an integer: invalid syntax"
                .to_string())
        );
    }

    // gpus_test.go: TestGpusOptOpts
    #[test]
    fn gpus_opt_reads_a_driver_capabilities_and_options() {
        for value in [
            r#"driver=nvidia,"capabilities=compute,utility","options=foo=bar,baz=qux""#,
            r#"1,driver=nvidia,"capabilities=compute,utility","options=foo=bar,baz=qux""#,
            r#"count=1,driver=nvidia,"capabilities=compute,utility","options=foo=bar,baz=qux""#,
            r#"driver=nvidia,"capabilities=compute,utility","options=foo=bar,baz=qux",count=1"#,
        ] {
            let mut g = GpuOpts::default();
            g.set(value).unwrap_or_else(|err| panic!("{value}: {err}"));
            assert_eq!(g.value().len(), 1, "{value}");
            assert_eq!(
                g.value()[0],
                DeviceRequest {
                    driver: "nvidia".to_string(),
                    count: 1,
                    // "gpu" is appended to whatever was asked for.
                    capabilities: vec![vec![
                        "compute".to_string(),
                        "utility".to_string(),
                        "gpu".to_string()
                    ]],
                    options: map_of(&[("foo", "bar"), ("baz", "qux")]),
                    device_ids: Vec::new(),
                },
                "{value}"
            );
        }
    }

    // gpus_test.go has no table for these, but the count default and the
    // duplicate-key rule are easy to get wrong, so they are pinned here.
    #[test]
    fn gpus_opt_pins_the_count_once_a_device_is_named() {
        let mut g = GpuOpts::default();
        g.set("device=0").expect("a valid request");
        assert_eq!(
            g.value()[0],
            DeviceRequest {
                device_ids: vec!["0".to_string()],
                count: 0,
                capabilities: vec![vec!["gpu".to_string()]],
                ..Default::default()
            }
        );
        assert_eq!(
            g.set("count=1,count=2"),
            Err("gpu request key 'count' can be specified only once".to_string())
        );
        // A bare key *is* the count, so it occupies the same slot.
        assert_eq!(
            g.set("1,count=1"),
            Err("gpu request key 'count' can be specified only once".to_string())
        );
        // No emptiness check of its own, unlike --mount.
        assert_eq!(
            g.set("options="),
            Err("failed to read gpu options: EOF".to_string())
        );
        assert_eq!(g.set(""), Err("EOF".to_string()));
        assert_eq!(g.type_name(), "gpu-request");
    }

    #[test]
    fn gpus_opt_string_prints_a_device_request_as_go_does() {
        let mut g = GpuOpts::default();
        g.set(r#"driver=nvidia,"capabilities=compute,utility","options=baz=qux,foo=bar""#)
            .expect("a valid request");
        assert_eq!(
            g.to_string(),
            "{nvidia 1 [] [[compute utility gpu]] map[baz:qux foo:bar]}"
        );
        g.set("all").expect("a valid request");
        assert_eq!(
            g.to_string(),
            "{nvidia 1 [] [[compute utility gpu]] map[baz:qux foo:bar]}, { -1 [] [[gpu]] map[]}"
        );
    }
}
