//! Docker's `run` option vocabulary: the validators and small parsers that turn
//! a workflow's `options:` string into what the daemon is asked for.
//!
//! act borrows this file from `docker/cli`'s `command/container/opts.go` rather
//! than implementing it, which is why the port is a port and not a rewrite.
//! Everything here is **pure**: no daemon, no socket, no side effects. That is
//! not a simplification — it is the reason 24 of the 27 upstream test functions
//! in `docker_cli_test.go` can be taken as-is, because they only ever exercise
//! the conversion from a flag string to a `HostConfig`.
//!
//! The daemon-facing half — `create`, `start`, `exec`, `pull` — is a separate
//! module, because it cannot be tested without a running Docker.
//!
//! # The validators are the contract, and their *error messages* are part of it
//!
//! These functions return the offending value alongside the error, exactly as
//! upstream does, and several of the tests assert on the message text verbatim:
//! `"./ is not an absolute path"`, `"bad mode specified: ro"`,
//! `"invalid logging opts for driver none"`. A user who mistypes `--device`
//! sees that string, so it is reproduced rather than paraphrased.
//!
//! # `path.Clean`, not `std::path`
//!
//! [`validate_linux_path`] operates on a **container** path, which is slash
//! separated on every host OS including Windows. `std::path` would treat a
//! backslash as a separator on Windows and collapse a `..` that must survive,
//! so [`slash_clean`] and [`slash_is_abs`] are literal Go `path.Clean` /
//! `path.IsAbs` ports — lexical, never touching a filesystem, because the
//! path being cleaned is one that does not exist yet.
//!
//! # Two tests upstream only run on Linux
//!
//! `TestParseDevice` and `TestValidateDevice` begin with
//! `skip.If(t, runtime.GOOS != "linux")` and a comment saying Windows and macOS
//! validate server-side. This port calls them with `"linux"` as the target
//! daemon's OS rather than the host's, so the tables run **everywhere** — the
//! same assertions, checked on a macOS build computer too. That is strictly more
//! coverage than upstream gets, and the only way the Linux path would otherwise
//! go untested on the platform CTOX is developed on.

use std::collections::{BTreeMap, BTreeSet};

/// A device mapped from the host into the container.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct DeviceMapping {
    /// Where the device is on the host.
    #[serde(rename = "PathOnHost")]
    pub path_on_host: String,
    /// Where it appears inside the container.
    #[serde(rename = "PathInContainer")]
    pub path_in_container: String,
    /// The cgroup access: a combination of `r`, `w` and `m`.
    #[serde(rename = "CgroupPermissions")]
    pub cgroup_permissions: String,
}

/// The built-in default seccomp profile.
pub const SECCOMP_PROFILE_DEFAULT: &str = "builtin";
/// The seccomp profile name meaning "no profile".
pub const SECCOMP_PROFILE_UNCONFINED: &str = "unconfined";

/// `validateAttach`: is this a stream the container can attach to?
///
/// The value is **lowercased** on the way through, which is what lets
/// `--attach=STDIN` work; the caller gets the lowercase form back.
///
/// The error is a bare sentence with no reference to the offending value:
/// `pflag` wraps a validator's message as
/// `invalid argument "x" for "-a, --attach" flag: <message>`, so a prefix here
/// would read `invalid argument "x" for "-a, --attach" flag: x: <message>`.
/// Upstream's expected string is asserted in `TestParseRunWithInvalidArgs` and
/// has no such repetition.
pub fn validate_attach(val: &str) -> Result<String, String> {
    let lowered = val.to_lowercase();
    if matches!(lowered.as_str(), "stdin" | "stdout" | "stderr") {
        return Ok(lowered);
    }
    Err("valid streams are STDIN, STDOUT and STDERR".to_string())
}

/// `convertToStandardNotation`: rewrite `-p` shorthand as `published:target/proto`.
///
/// A publish spec without `=` is already in standard notation and passes
/// through. With `=`, it is a comma-separated `key=value` list, and the two keys
/// that matter are `published` and `target` — `protocol` defaults to `tcp`.
///
/// # A missing key is not an error
///
/// `params["published"]` on a Go map yields `""` for an absent key, so
/// `-p target=80` becomes `":80/tcp"` rather than failing. That is upstream's
/// behaviour and it is preserved: the daemon rejects it, and a pull that would
/// otherwise work keeps working.
pub fn convert_to_standard_notation(ports: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(ports.len());
    for publish in ports {
        if !publish.contains('=') {
            out.push(publish.clone());
            continue;
        }
        let mut params: BTreeMap<&str, &str> = BTreeMap::new();
        params.insert("protocol", "tcp");
        for param in publish.split(',') {
            let Some((key, value)) = param.split_once('=') else {
                return Err(format!(
                    "invalid publish opts format (should be name=value but got '{param}')"
                ));
            };
            if key.is_empty() {
                return Err(format!(
                    "invalid publish opts format (should be name=value but got '{param}')"
                ));
            }
            params.insert(key, value);
        }
        // A Go map read of an absent key is the empty string, not a panic.
        let published = params.get("published").copied().unwrap_or_default();
        let target = params.get("target").copied().unwrap_or_default();
        let protocol = params.get("protocol").copied().unwrap_or_default();
        out.push(format!("{published}:{target}/{protocol}"));
    }
    Ok(out)
}

/// `opts.ConvertKVStringsToMap`: `["a=1", "b=2"]` to a map.
///
/// A key with no `=` maps to the empty string, and a repeated key **last wins**,
/// because each pair overwrites the previous entry in the map.
pub fn convert_kv_strings_to_map(items: &[String]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for item in items {
        let (key, value) = item.split_once('=').unwrap_or((item.as_str(), ""));
        map.insert(key.to_string(), value.to_string());
    }
    map
}

/// `parseLoggingOpts`.
///
/// The `none` driver takes no options, and one arriving anyway is a user error
/// worth reporting — silently dropping it would hide a typo.
pub fn parse_logging_opts(
    logging_driver: &str,
    logging_opts: &[String],
) -> Result<BTreeMap<String, String>, String> {
    if logging_driver == "none" && !logging_opts.is_empty() {
        return Err(format!("invalid logging opts for driver {logging_driver}"));
    }
    Ok(convert_kv_strings_to_map(logging_opts))
}

/// `parseStorageOpts`: `size=10G` and friends into a map.
///
/// Unlike the security options, a storage option without a value is always an
/// error.
pub fn parse_storage_opts(storage_opts: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    for option in storage_opts {
        let Some((key, value)) = option.split_once('=') else {
            return Err("invalid storage option".to_string());
        };
        map.insert(key.to_string(), value.to_string());
    }
    Ok(map)
}

/// `parseSystemPaths`: pull `systempaths=unconfined` out of the security options.
///
/// That option is handled **client-side** and must not reach the daemon, so it
/// is removed here and turned into two empty path lists. Any other value
/// (`systempaths=unknown`) is left alone for the daemon to reject.
///
/// Go writes into the caller's backing array (`filtered = securityOpts[:0]`);
/// this builds a new vector, which is the same observable result without the
/// aliasing.
pub fn parse_system_paths(security_opts: &[String]) -> (Vec<String>, Option<Vec<String>>, Option<Vec<String>>) {
    let mut filtered = Vec::with_capacity(security_opts.len());
    let mut masked_paths = None;
    let mut readonly_paths = None;
    for opt in security_opts {
        if opt == "systempaths=unconfined" {
            masked_paths = Some(Vec::new());
            readonly_paths = Some(Vec::new());
        } else {
            filtered.push(opt.clone());
        }
    }
    (filtered, masked_paths, readonly_paths)
}

/// `parseSecurityOpts`: validate the security options and inline a seccomp file.
///
/// Two spellings are accepted, `=` and `:`, because Docker's own CLI accepts
/// both. `no-new-privileges` is the one option that takes no value.
///
/// A `seccomp=<name>` that is not `builtin` or `unconfined` names a **file**,
/// whose contents are read and compacted to JSON before being sent — the daemon
/// takes the profile, not a path.
pub fn parse_security_opts(security_opts: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(security_opts.len());
    for opt in security_opts {
        let (mut key, mut value) = split_value(opt, '=');
        if value.is_none() && key != "no-new-privileges" {
            let (k, v) = split_value(opt, ':');
            key = k;
            value = v;
        }
        if value.as_deref().unwrap_or("").is_empty() && key != "no-new-privileges" {
            return Err(format!("invalid --security-opt: \"{opt}\""));
        }
        let value = value.unwrap_or_default();
        if key == "seccomp" {
            match value.as_str() {
                SECCOMP_PROFILE_DEFAULT | SECCOMP_PROFILE_UNCONFINED => out.push(opt.clone()),
                // May be a filename, in which case the profile's content is
                // sent if it is valid JSON.
                path => {
                    let contents = std::fs::read_to_string(path).map_err(|err| {
                        format!("opening seccomp profile ({path}) failed: {err}")
                    })?;
                    let compacted = serde_json::from_str::<serde_json::Value>(&contents)
                        .map_err(|err| {
                            format!("compacting json for seccomp profile ({path}) failed: {err}")
                        })?;
                    out.push(format!("seccomp={}", compact_json(&compacted)));
                }
            }
        } else {
            out.push(opt.clone());
        }
    }
    Ok(out)
}

/// `strings.Cut`, with "absent" distinguishable from "empty".
///
/// Upstream needs the difference: `security-opt:foo=` is rejected because the
/// value is empty, while `no-new-privileges` is accepted with no value at all.
fn split_value(value: &str, separator: char) -> (String, Option<String>) {
    match value.split_once(separator) {
        Some((key, rest)) => (key.to_string(), Some(rest.to_string())),
        None => (value.to_string(), None),
    }
}

/// `json.Compact`: the same JSON with no insignificant whitespace.
fn compact_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// `parseDevice`: a `--device` value, for the daemon's operating system.
pub fn parse_device(device: &str, server_os: &str) -> Result<DeviceMapping, String> {
    match server_os {
        "linux" => parse_linux_device(device),
        // Windows has no device mapping, so the value is passed through as-is
        // and validated by the daemon.
        "windows" => Ok(DeviceMapping {
            path_on_host: device.to_string(),
            ..Default::default()
        }),
        other => Err(format!("unknown server OS: {other}")),
    }
}

/// `parseLinuxDevice`: `host[:container][:mode]`, defaulting the container path
/// to the host path and the mode to `rwm`.
///
/// The `fallthrough` chain is load-bearing. With three parts the **third** is
/// taken as the mode, and only then is the second examined — so
/// `/dev/snd:rw:extra` ends up with mode `rw` and container path `/dev/snd`,
/// not with container path `rw`.
pub fn parse_linux_device(device: &str) -> Result<DeviceMapping, String> {
    let mut dst = String::new();
    let mut permissions = "rwm".to_string();

    // At most three parts are expected; the limit of four detects an overflow.
    let parts: Vec<&str> = device.splitn(4, ':').collect();
    match parts.len() {
        3 => permissions = parts[2].to_string(),
        2 | 1 => {}
        _ => return Err(format!("invalid device specification: {device}")),
    }
    if parts.len() >= 2 {
        if valid_device_mode(parts[1]) {
            permissions = parts[1].to_string();
        } else {
            dst = parts[1].to_string();
        }
    }
    let src = parts[0].to_string();

    if dst.is_empty() {
        dst = src.clone();
    }
    Ok(DeviceMapping {
        path_on_host: src,
        path_in_container: dst,
        cgroup_permissions: permissions,
    })
}

/// `validDeviceMode`: a non-empty, duplicate-free subset of `r`, `w`, `m`.
///
/// The duplicate check is what rejects `rr`, and it is why `path:ro` is read as
/// a host path plus a *container* path rather than a host path plus a mode —
/// `o` is not a legal mode character.
pub fn valid_device_mode(mode: &str) -> bool {
    if mode.is_empty() {
        return false;
    }
    let mut seen = BTreeSet::new();
    for character in mode.chars() {
        if !matches!(character, 'r' | 'w' | 'm') {
            return false;
        }
        if !seen.insert(character) {
            return false;
        }
    }
    true
}

/// `validateDeviceCgroupRule`: `type major:minor mode`, e.g. `c 1:3 mr`.
pub fn validate_device_cgroup_rule(val: &str) -> Result<String, String> {
    let pattern = r"^[acb] ([0-9]+|\*):([0-9]+|\*) [rwm]{1,3}$";
    let ok = regex::Regex::new(pattern)
        .expect("the cgroup rule pattern is a literal")
        .is_match(val);
    if ok {
        return Ok(val.to_string());
    }
    Err(format!("invalid device cgroup format '{val}'"))
}

/// `validateDevice`: a `--device` value, checked per the daemon's operating system.
pub fn validate_device(val: &str, server_os: &str) -> Result<String, String> {
    match server_os {
        "linux" => validate_linux_path(val, valid_device_mode),
        // Windows does validation entirely server-side.
        "windows" => Ok(val.to_string()),
        other => Err(format!("unknown server OS: {other}")),
    }
}

/// `validateLinuxPath`: `host-dir:container-path[:mode]`, both paths absolute.
///
/// The mode is detected rather than assumed, which is what makes the ambiguous
/// two-part form decidable: in `relative:/absolute-path` the second component is
/// not a valid mode, so it is a path; in `hostPath:/containerPath:r` it is, so it
/// is a mode.
pub fn validate_linux_path(
    val: &str,
    validator: fn(&str) -> bool,
) -> Result<String, String> {
    if val.matches(':').count() > 2 {
        return Err(format!("bad format for path: {val}"));
    }
    let split: Vec<&str> = val.splitn(3, ':').collect();
    if split[0].is_empty() {
        return Err(format!("bad format for path: {val}"));
    }

    let container_path: String;
    let value: String = match split.len() {
        1 => {
            container_path = split[0].to_string();
            slash_clean(&container_path)
        }
        2 => {
            if validator(split[1]) {
                container_path = split[0].to_string();
                format!("{}:{}", slash_clean(&container_path), split[1])
            } else {
                container_path = split[1].to_string();
                format!("{}:{}", split[0], slash_clean(&container_path))
            }
        }
        3 => {
            container_path = split[1].to_string();
            if !validator(split[2]) {
                return Err(format!("bad mode specified: {}", split[2]));
            }
            format!("{}:{}:{}", split[0], container_path, split[2])
        }
        _ => unreachable!("splitn(3) yields at most three parts"),
    };

    if !slash_is_abs(&container_path) {
        return Err(format!("{container_path} is not an absolute path"));
    }
    Ok(value)
}

/// Go's `path.Clean`, for a slash-separated path.
///
/// Lexical, like the original: the path being cleaned is a container path that
/// does not exist yet, so it must not touch a filesystem. It differs from
/// `std::path` in two ways that matter — a backslash is an ordinary character on
/// every platform here, and `..` is collapsed even past the root, where it is
/// simply dropped.
pub fn slash_clean(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let rooted = path.starts_with('/');
    let bytes = path.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    if rooted {
        out.push(b'/');
    }
    // `dotdot` is the length of `out` up to which `..` may not rewind.
    let mut dotdot = if rooted { 1 } else { 0 };
    let mut index = if rooted { 1 } else { 0 };

    while index < bytes.len() {
        if bytes[index] == b'/' {
            // Empty path element.
            index += 1;
        } else if bytes[index] == b'.'
            && (index + 1 == bytes.len() || bytes[index + 1] == b'/')
        {
            // `.` element.
            index += 1;
        } else if bytes[index] == b'.'
            && index + 1 < bytes.len()
            && bytes[index + 1] == b'.'
            && (index + 2 == bytes.len() || bytes[index + 2] == b'/')
        {
            index += 2;
            if out.len() > dotdot {
                // Go rewinds a *write pointer* over a buffer that keeps its
                // bytes, and the final length is where the pointer stopped —
                // which sits just *after* the separator, so the separator ends
                // up excluded. Reproduced with an index rather than `pop()`,
                // because `pop()` cannot look at the element it just removed,
                // and truncating to the separator's own position instead would
                // turn `/a/b/..` into `/a/` and `/a/b/../..` into `.`.
                let mut w = out.len() - 1;
                while w > dotdot && out[w] != b'/' {
                    w -= 1;
                }
                out.truncate(w);
            } else if !rooted {
                // Relative paths keep leading `..`; absolute ones drop it.
                if !out.is_empty() {
                    out.push(b'/');
                }
                out.extend_from_slice(b"..");
                dotdot = out.len();
            }
        } else {
            if (rooted && out.len() != 1) || (!rooted && !out.is_empty()) {
                out.push(b'/');
            }
            while index < bytes.len() && bytes[index] != b'/' {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    if out.is_empty() {
        return ".".to_string();
    }
    // A path is only valid UTF-8 if it was valid UTF-8 on the way in.
    String::from_utf8(out).unwrap_or_else(|_| ".".to_string())
}

/// Go's `path.IsAbs`: a leading slash, and nothing else.
pub fn slash_is_abs(path: &str) -> bool {
    path.starts_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    // docker_cli_test.go: TestValidateAttach
    #[test]
    fn an_attach_stream_is_lowercased() {
        for attach in ["stdin", "stdout", "stderr", "STDIN", "STDOUT", "STDERR"] {
            assert_eq!(
                validate_attach(attach).expect("a valid stream"),
                attach.to_lowercase(),
                "validateAttach({attach:?})",
            );
        }
        // The message is a bare sentence. `pflag` prepends the flag and the
        // offending value, and `TestParseRunWithInvalidArgs` asserts the whole
        // composed string, so a prefix here would be visible there.
        assert_eq!(
            validate_attach("invalid").expect_err("not a stream"),
            "valid streams are STDIN, STDOUT and STDERR",
        );
    }

    // docker_cli_test.go: TestConvertToStandardNotation
    #[test]
    fn publish_specs_are_rewritten_in_standard_notation() {
        let valid: &[(&str, &[&str])] = &[
            ("20:10/tcp", &["target=10,published=20"]),
            ("40:30", &["40:30"]),
            ("20:20 80:4444", &["20:20", "80:4444"]),
            (
                "1500:2500/tcp 1400:1300",
                &["target=2500,published=1500", "1400:1300"],
            ),
            (
                "1500:200/tcp 90:80/tcp",
                &["published=1500,target=200", "target=80,published=90"],
            ),
        ];
        for (want, ports) in valid {
            let inputs: Vec<String> = ports.iter().map(|p| p.to_string()).collect();
            let converted = convert_to_standard_notation(&inputs).expect("converted");
            assert_eq!(
                converted,
                want.split(' ').map(str::to_string).collect::<Vec<_>>(),
                "for {ports:?}",
            );
        }
    }

    #[test]
    fn a_publish_spec_without_a_value_is_rejected() {
        for ports in [
            ["published=1500,target:444"],
            ["published=1500,444"],
            ["published=1500,target,444"],
        ] {
            let ports: Vec<String> = ports.iter().map(|p| p.to_string()).collect();
            let err = convert_to_standard_notation(&ports)
                .expect_err("a param without '=' is an error");
            assert!(err.contains("should be name=value"), "got {err:?}");
        }
    }

    /// A spec naming only a target still converts, to `":80/tcp"`, because Go's
    /// map read of an absent key is the empty string rather than an error.
    #[test]
    fn a_publish_spec_without_a_published_port_still_converts() {
        assert_eq!(
            convert_to_standard_notation(&["target=80".to_string()]).expect("converted"),
            vec![":80/tcp".to_string()],
        );
    }

    // docker_cli_test.go: TestParseDevice
    #[test]
    fn a_device_maps_to_a_host_and_a_container_path() {
        let table: &[(&str, &str, &str, &str)] = &[
            ("/dev/snd", "/dev/snd", "/dev/snd", "rwm"),
            ("/dev/snd:rw", "/dev/snd", "/dev/snd", "rw"),
            ("/dev/snd:/something", "/dev/snd", "/something", "rwm"),
            ("/dev/snd:/something:rw", "/dev/snd", "/something", "rw"),
        ];
        for (input, host, container, mode) in table {
            let mapping = parse_device(input, "linux").expect("a mapping");
            assert_eq!(mapping.path_on_host, *host, "host for {input}");
            assert_eq!(mapping.path_in_container, *container, "container for {input}");
            assert_eq!(mapping.cgroup_permissions, *mode, "mode for {input}");
        }
    }

    /// The `fallthrough` chain: with three parts the *second* is preferred as the
    /// mode when it can be one, even though the third was read first.
    #[test]
    fn a_three_part_device_prefers_a_valid_mode_in_the_second_slot() {
        let mapping = parse_linux_device("/dev/snd:rw:extra").expect("a mapping");
        assert_eq!(mapping.cgroup_permissions, "rw");
        // The container path falls back to the host path, because `rw` was
        // taken as the mode rather than as a path.
        assert_eq!(mapping.path_in_container, "/dev/snd");
    }

    #[test]
    fn a_four_part_device_is_rejected() {
        let err = parse_linux_device("/a:b:c:d").expect_err("too many parts");
        assert!(err.contains("invalid device specification"), "got {err:?}");
    }

    /// Windows passes the value through and lets the daemon judge it; anything
    /// else is an error naming the operating system.
    #[test]
    fn the_device_rules_depend_on_the_daemons_operating_system() {
        let mapping = parse_device("vendor.com/class=name", "windows").expect("pass-through");
        assert_eq!(mapping.path_on_host, "vendor.com/class=name");
        assert!(
            mapping.path_in_container.is_empty(),
            "Windows has no in-container path",
        );
        let err = parse_device("/dev/snd", "darwin").expect_err("no such daemon");
        assert_eq!(err, "unknown server OS: darwin");
    }

    #[test]
    fn a_device_mode_is_a_duplicate_free_subset_of_rwm() {
        for mode in ["r", "w", "m", "rw", "mrw", "rwm"] {
            assert!(valid_device_mode(mode), "{mode} should be valid");
        }
        for mode in ["", "o", "ro", "rr", "rwx", "R"] {
            assert!(!valid_device_mode(mode), "{mode} should be invalid");
        }
    }

    // docker_cli_test.go: TestValidateDevice — upstream skips this on a
    // non-Linux host; the tables are run against a Linux daemon everywhere.
    #[test]
    fn a_valid_device_path_is_accepted() {
        for path in [
            "/home",
            "/home:/home",
            "/home:/something/else",
            "/with space",
            "/home:/with space",
            "relative:/absolute-path",
            "hostPath:/containerPath:r",
            "/hostPath:/containerPath:rw",
            "/hostPath:/containerPath:mrw",
        ] {
            validate_device(path, "linux")
                .unwrap_or_else(|err| panic!("validateDevice({path:?}) should succeed: {err}"));
        }
    }

    /// The message is asserted verbatim, because it is what the user sees.
    #[test]
    fn an_invalid_device_path_names_the_problem() {
        let table: &[(&str, &str)] = &[
            ("", "bad format for path: "),
            ("./", "./ is not an absolute path"),
            ("../", "../ is not an absolute path"),
            ("/:../", "../ is not an absolute path"),
            ("/:path", "path is not an absolute path"),
            (":", "bad format for path: :"),
            ("/tmp:", " is not an absolute path"),
            (":test", "bad format for path: :test"),
            (":/test", "bad format for path: :/test"),
            ("tmp:", " is not an absolute path"),
            (":test:", "bad format for path: :test:"),
            ("::", "bad format for path: ::"),
            (":::", "bad format for path: :::"),
            ("/tmp:::", "bad format for path: /tmp:::"),
            (":/tmp::", "bad format for path: :/tmp::"),
            ("path:ro", "ro is not an absolute path"),
            ("path:rr", "rr is not an absolute path"),
            ("a:/b:ro", "bad mode specified: ro"),
            ("a:/b:rr", "bad mode specified: rr"),
        ];
        for (path, want) in table {
            let err = validate_device(path, "linux")
                .expect_err(&format!("validateDevice({path:?}) should have failed"));
            assert_eq!(&err, want, "message for {path:?}");
        }
    }

    // docker_cli_test.go: TestParseSystemPaths
    #[test]
    fn systempaths_unconfined_is_removed_before_the_daemon_sees_it() {
        // Not set: everything passes through and no path list is produced.
        let (out, masked, readonly) = parse_system_paths(&[]);
        assert!(out.is_empty());
        assert!(masked.is_none() && readonly.is_none());

        // Not set, other options preserved verbatim.
        let input: Vec<String> = [
            "seccomp=unconfined",
            "apparmor=unconfined",
            "label=user:USER",
            "foo=bar",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let (out, masked, readonly) = parse_system_paths(&input);
        assert_eq!(out, input);
        assert!(masked.is_none() && readonly.is_none());

        // Unconfined: removed, and both path lists are emptied.
        let (out, masked, readonly) = parse_system_paths(&["systempaths=unconfined".to_string()]);
        assert!(out.is_empty());
        assert_eq!(masked, Some(Vec::new()));
        assert_eq!(readonly, Some(Vec::new()));

        // Unconfined alongside other options: only the option is removed.
        let input: Vec<String> = ["foo=bar", "bar=baz", "systempaths=unconfined"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (out, masked, readonly) = parse_system_paths(&input);
        assert_eq!(out, vec!["foo=bar".to_string(), "bar=baz".to_string()]);
        assert_eq!(masked, Some(Vec::new()));
        assert_eq!(readonly, Some(Vec::new()));

        // An unknown value is not `unconfined`, so it stays for the daemon.
        let input: Vec<String> = ["foo=bar", "systempaths=unknown", "bar=baz"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (out, masked, readonly) = parse_system_paths(&input);
        assert_eq!(out, input);
        assert!(masked.is_none() && readonly.is_none());
    }

    // docker_cli_test.go: TestParseLoggingOpts
    #[test]
    fn the_none_driver_takes_no_options() {
        let err = parse_logging_opts("none", &["anything".to_string()]).expect_err("rejected");
        assert_eq!(err, "invalid logging opts for driver none");

        let map = parse_logging_opts("syslog", &["something".to_string()]).expect("accepted");
        assert_eq!(map.len(), 1);
        assert_eq!(map["something"], "");

        // The `none` driver with no options is fine.
        assert!(parse_logging_opts("none", &[]).expect("accepted").is_empty());
    }

    /// A repeated key last wins, and a key with no `=` maps to the empty string.
    #[test]
    fn a_repeated_option_key_keeps_the_last_value() {
        let map = parse_logging_opts(
            "json-file",
            &["max-size=10m".to_string(), "max-size=20m".to_string()],
        )
        .expect("accepted");
        assert_eq!(map["max-size"], "20m");
    }

    /// `no-new-privileges` is the only option that needs no value, and `=`
    /// and `:` are both accepted spellings.
    #[test]
    fn a_security_option_needs_a_value_unless_it_is_no_new_privileges() {
        assert!(parse_security_opts(&["no-new-privileges".to_string()]).is_ok());
        assert!(parse_security_opts(&["apparmor=unconfined".to_string()]).is_ok());
        assert!(parse_security_opts(&["label:user:USER".to_string()]).is_ok());

        // An empty value is still empty, whichever separator introduced it.
        assert!(parse_security_opts(&["apparmor=".to_string()]).is_err());
        assert!(parse_security_opts(&["apparmor".to_string()]).is_err());
    }

    /// `seccomp` takes either a built-in profile name or a file whose contents
    /// are inlined, compacted.
    #[test]
    fn a_seccomp_profile_is_inlined_from_its_file() {
        for builtin in ["builtin", "unconfined"] {
            let out = parse_security_opts(&[format!("seccomp={builtin}")]).expect("accepted");
            assert_eq!(out, vec![format!("seccomp={builtin}")]);
        }

        let dir = tempfile::tempdir().expect("a temporary directory");
        let profile = dir.path().join("profile.json");
        std::fs::write(&profile, "{\n  \"defaultAction\": \"SCMP_ACT_ALLOW\"\n}\n")
            .expect("written");
        let out = parse_security_opts(&[format!("seccomp={}", profile.display())])
            .expect("accepted");
        assert_eq!(
            out,
            vec![r#"seccomp={"defaultAction":"SCMP_ACT_ALLOW"}"#.to_string()],
            "the profile is inlined and compacted",
        );

        // A missing file, and a file that is not JSON, are both errors.
        assert!(parse_security_opts(&["seccomp=/nope/missing.json".to_string()]).is_err());
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, "{not json").expect("written");
        assert!(parse_security_opts(&[format!("seccomp={}", bad.display())]).is_err());
    }

    #[test]
    fn a_storage_option_without_a_value_is_rejected() {
        assert!(parse_storage_opts(&["size".to_string()]).is_err());
        let map = parse_storage_opts(&["size=10G".to_string(), "dm.basesize=20G".to_string()])
            .expect("accepted");
        assert_eq!(map["size"], "10G");
        assert_eq!(map["dm.basesize"], "20G");
    }

    #[test]
    fn a_device_cgroup_rule_is_type_major_minor_mode() {
        for rule in ["c 1:3 mr", "a *:* rwm", "b 8:0 r"] {
            assert_eq!(
                validate_device_cgroup_rule(rule).expect("a rule"),
                rule,
            );
        }
        for rule in ["", "c 1:3", "d 1:3 r", "c 1-3 r", "c 1:3 rwmw"] {
            assert!(validate_device_cgroup_rule(rule).is_err(), "{rule} should fail");
        }
    }

    /// Go's `path.Clean`, including the two behaviours `std::path` does not
    /// share: a backslash is an ordinary character, and `..` past the root is
    /// dropped rather than escaping.
    #[test]
    fn the_slash_clean_matches_go() {
        let table: &[(&str, &str)] = &[
            ("", "."),
            (".", "."),
            ("..", ".."),
            ("/..", "/"),
            ("/../..", "/"),
            ("/", "/"),
            ("//", "/"),
            ("/a/b", "/a/b"),
            ("/a//b", "/a/b"),
            ("/a/./b", "/a/b"),
            ("/a/b/..", "/a"),
            ("/a/b/../..", "/"),
            ("a/b/../c", "a/c"),
            ("a/../b", "b"),
            ("../a", "../a"),
            ("../../a", "../../a"),
            ("/a/b/../c", "/a/c"),
            ("./a", "a"),
            // A backslash is an ordinary character, not a separator.
            (r"a\b", r"a\b"),
            // Measured under Go 1.26 as a spot check on a longer rewind.
            ("/a/b/c/../../d", "/a/d"),
        ];
        for (input, want) in table {
            assert_eq!(slash_clean(input), *want, "clean({input:?})");
        }
    }

    /// The path under validation is a *container* path: slash-separated on every
    /// host, including Windows, where `std::path` would split on a backslash.
    #[test]
    fn a_container_path_is_slash_separated_on_every_host() {
        // A backslash in the host part is an ordinary character, so the whole
        // `/host\path` survives and the second field is read as the container
        // path. `std::path` on Windows would have split it at the backslash.
        assert_eq!(
            validate_linux_path(r"/host\path:/container:r", valid_device_mode)
                .expect("a mapping"),
            r"/host\path:/container:r",
        );
        // The container path itself is judged by the slash rule alone: `\a` is
        // not absolute on any host, because it starts with a backslash.
        let err = validate_linux_path(r"/host:\container", valid_device_mode)
            .expect_err("a backslash is not a root");
        assert_eq!(err, r"\container is not an absolute path");

        // A Windows drive letter is a third colon, which the format rejects
        // before anything else is considered.
        assert_eq!(
            validate_linux_path(r"C:\host:/container:r", valid_device_mode)
                .expect_err("three colons"),
            r"bad format for path: C:\host:/container:r",
        );

        assert!(slash_is_abs("/a"));
        assert!(!slash_is_abs("a"));
        assert!(!slash_is_abs(""));
    }
}
