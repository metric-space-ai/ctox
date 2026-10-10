//! `docker run`'s flag table: the ~100 flags act accepts in a workflow's
//! `options:`.
//!
//! This is `addFlags` from `pkg/container/docker_cli.go`, which act itself
//! borrowed from `docker/cli`'s `command/container/opts.go`. The declarations
//! are reproduced rather than narrowed, because the acceptance criterion for
//! this port is that upstream's tests run: they pass `docker` flags a CTOX
//! user's workflow may well contain, and a table missing one turns a working
//! workflow into a parse error.
//!
//! # Three flags exist twice, on purpose
//!
//! `--net`/`--network`, `--net-alias`/`--network-alias` and
//! `--dns-opt`/`--dns-option` are all registered twice, with one spelling
//! hidden. Dropping the hidden spelling would be a silent behaviour change for
//! a workflow that predates the preferred one.
//!
//! # `kernel-memory` is registered and does nothing
//!
//! Upstream keeps a stub so that an old command line still *parses* and gets a
//! deprecation notice rather than an "unknown flag" error. It is declared here
//! for the same reason, and a value passed to it is accepted and discarded.

use super::pflags::{FlagDef, FlagKind, FlagSet, ThrottleKind};
use super::{docker_opts, docker_opts_types as types};

/// Every flag `docker run` accepts, as act declares it.
pub fn run_flag_set() -> FlagSet {
    use FlagKind::*;
    FlagSet::new(vec![
        // ── General purpose ───────────────────────────────────────────────
        FlagDef::new("attach", List(Some(docker_opts::validate_attach))).short('a'),
        FlagDef::new(
            "device-cgroup-rule",
            List(Some(docker_opts::validate_device_cgroup_rule)),
        ),
        // Devices can only be validated once the daemon's OS is known, so no
        // validator here — `parse` does it late, deliberately.
        FlagDef::new("device", List(None)),
        FlagDef::new("gpus", List(None)),
        FlagDef::new("env", List(Some(types::validate_env))).short('e'),
        FlagDef::new("env-file", List(None)),
        FlagDef::new("entrypoint", Text),
        FlagDef::new("group-add", List(None)),
        FlagDef::new("hostname", Text).short('h'),
        FlagDef::new("domainname", Text),
        FlagDef::new("interactive", Bool).short('i'),
        FlagDef::new("label", List(Some(types::validate_label))).short('l'),
        FlagDef::new("label-file", List(None)),
        FlagDef::new("read-only", Bool),
        FlagDef::new("restart", Text),
        FlagDef::new("stop-signal", Text),
        FlagDef::new("stop-timeout", Int),
        FlagDef::new("sysctl", Map(Some(types::validate_sysctl))),
        FlagDef::new("tty", Bool).short('t'),
        FlagDef::new("ulimit", Ulimit),
        FlagDef::new("user", Text).short('u'),
        FlagDef::new("workdir", Text).short('w'),
        FlagDef::new("rm", Bool),
        FlagDef::new("annotation", Map(None)),
        // Deprecated upstream, but it must still parse.
        FlagDef::new("kernel-memory", MemBytes),
        // ── Security ──────────────────────────────────────────────────────
        FlagDef::new("cap-add", List(None)),
        FlagDef::new("cap-drop", List(None)),
        FlagDef::new("privileged", Bool),
        FlagDef::new("security-opt", List(None)),
        FlagDef::new("userns", Text),
        FlagDef::new("cgroupns", Text),
        // ── Network and port publishing ───────────────────────────────────
        FlagDef::new("add-host", List(Some(types::validate_extra_host))),
        FlagDef::new("dns", List(Some(types::validate_ip_address))),
        FlagDef::new("dns-opt", List(None)),
        FlagDef::new("dns-option", List(None)),
        FlagDef::new("dns-search", List(Some(types::validate_dns_search))),
        FlagDef::new("expose", List(None)),
        FlagDef::new("ip", Ip),
        FlagDef::new("ip6", Ip),
        FlagDef::new("link", List(Some(types::validate_link))),
        FlagDef::new("link-local-ip", List(None)),
        FlagDef::new("mac-address", Text),
        FlagDef::new("publish", List(None)).short('p'),
        FlagDef::new("publish-all", Bool).short('P'),
        FlagDef::new("net", List(None)),
        FlagDef::new("network", List(None)),
        FlagDef::new("net-alias", List(None)),
        FlagDef::new("network-alias", List(None)),
        // ── Logging and storage ───────────────────────────────────────────
        FlagDef::new("log-driver", Text),
        FlagDef::new("volume-driver", Text),
        FlagDef::new("log-opt", List(None)),
        FlagDef::new("storage-opt", List(None)),
        FlagDef::new("tmpfs", List(None)),
        FlagDef::new("volumes-from", List(None)),
        FlagDef::new("volume", List(None)).short('v'),
        FlagDef::new("mount", List(None)),
        // ── Health checking ───────────────────────────────────────────────
        FlagDef::new("health-cmd", Text),
        FlagDef::new("health-interval", Duration),
        FlagDef::new("health-retries", Int),
        FlagDef::new("health-timeout", Duration),
        FlagDef::new("health-start-period", Duration),
        FlagDef::new("health-start-interval", Duration),
        FlagDef::new("no-healthcheck", Bool),
        // ── Resource management ───────────────────────────────────────────
        FlagDef::new("blkio-weight", Uint16),
        FlagDef::new("blkio-weight-device", WeightDevice),
        FlagDef::new("cidfile", Text),
        FlagDef::new("cpuset-cpus", Text),
        FlagDef::new("cpuset-mems", Text),
        FlagDef::new("cpu-count", Int64),
        FlagDef::new("cpu-percent", Int64),
        FlagDef::new("cpu-period", Int64),
        FlagDef::new("cpu-quota", Int64),
        FlagDef::new("cpu-rt-period", Int64),
        FlagDef::new("cpu-rt-runtime", Int64),
        FlagDef::new("cpu-shares", Int64).short('c'),
        FlagDef::new("cpus", NanoCpus),
        FlagDef::new("device-read-bps", ThrottleDevice(ThrottleKind::Bps)),
        FlagDef::new("device-read-iops", ThrottleDevice(ThrottleKind::Iops)),
        FlagDef::new("device-write-bps", ThrottleDevice(ThrottleKind::Bps)),
        FlagDef::new("device-write-iops", ThrottleDevice(ThrottleKind::Iops)),
        FlagDef::new("io-maxbandwidth", MemBytes),
        FlagDef::new("io-maxiops", Uint64),
        FlagDef::new("memory", MemBytes).short('m'),
        FlagDef::new("memory-reservation", MemBytes),
        FlagDef::new("memory-swap", MemSwapBytes),
        FlagDef::new("memory-swappiness", Int64),
        FlagDef::new("oom-kill-disable", Bool),
        FlagDef::new("oom-score-adj", Int),
        FlagDef::new("pids-limit", Int64),
        // ── Low-level execution ───────────────────────────────────────────
        FlagDef::new("cgroup-parent", Text),
        FlagDef::new("ipc", Text),
        FlagDef::new("isolation", Text),
        FlagDef::new("pid", Text),
        FlagDef::new("shm-size", MemBytes),
        FlagDef::new("uts", Text),
        FlagDef::new("runtime", Text),
        FlagDef::new("init", Bool),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::pflags::ParsedFlags;

    fn parse(args: &[&str]) -> ParsedFlags {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        run_flag_set()
            .parse(&args)
            .expect("the flag line should parse")
    }

    /// The table is the contract a workflow's `options:` is written against, so
    /// a missing flag is a parse error for a workflow that used to work.
    #[test]
    fn the_upstream_flag_set_is_complete() {
        let flags = run_flag_set();
        let names: Vec<&str> = flags.flags().iter().map(|f| f.long).collect();
        let expected = [
            "attach",
            "device-cgroup-rule",
            "device",
            "gpus",
            "env",
            "env-file",
            "entrypoint",
            "group-add",
            "hostname",
            "domainname",
            "interactive",
            "label",
            "label-file",
            "read-only",
            "restart",
            "stop-signal",
            "stop-timeout",
            "sysctl",
            "tty",
            "ulimit",
            "user",
            "workdir",
            "rm",
            "annotation",
            "kernel-memory",
            "cap-add",
            "cap-drop",
            "privileged",
            "security-opt",
            "userns",
            "cgroupns",
            "add-host",
            "dns",
            "dns-opt",
            "dns-option",
            "dns-search",
            "expose",
            "ip",
            "ip6",
            "link",
            "link-local-ip",
            "mac-address",
            "publish",
            "publish-all",
            "net",
            "network",
            "net-alias",
            "network-alias",
            "log-driver",
            "volume-driver",
            "log-opt",
            "storage-opt",
            "tmpfs",
            "volumes-from",
            "volume",
            "mount",
            "health-cmd",
            "health-interval",
            "health-retries",
            "health-timeout",
            "health-start-period",
            "health-start-interval",
            "no-healthcheck",
            "blkio-weight",
            "blkio-weight-device",
            "cidfile",
            "cpuset-cpus",
            "cpuset-mems",
            "cpu-count",
            "cpu-percent",
            "cpu-period",
            "cpu-quota",
            "cpu-rt-period",
            "cpu-rt-runtime",
            "cpu-shares",
            "cpus",
            "device-read-bps",
            "device-read-iops",
            "device-write-bps",
            "device-write-iops",
            "io-maxbandwidth",
            "io-maxiops",
            "memory",
            "memory-reservation",
            "memory-swap",
            "memory-swappiness",
            "oom-kill-disable",
            "oom-score-adj",
            "pids-limit",
            "cgroup-parent",
            "ipc",
            "isolation",
            "pid",
            "shm-size",
            "uts",
            "runtime",
            "init",
        ];
        for name in expected {
            assert!(names.contains(&name), "--{name} is missing from the table");
        }
        assert_eq!(names.len(), expected.len(), "the table grew unexpectedly");
    }

    /// Both spellings must exist, or a workflow written against the older one
    /// stops parsing.
    #[test]
    fn the_aliased_flags_are_both_declared() {
        let flags = run_flag_set();
        for (older, newer) in [
            ("net", "network"),
            ("net-alias", "network-alias"),
            ("dns-opt", "dns-option"),
        ] {
            for name in [older, newer] {
                assert!(
                    flags.flags().iter().any(|f| f.long == name),
                    "--{name} must stay declared",
                );
            }
        }
    }

    /// A deprecated flag must still parse, or an old command line turns into an
    /// "unknown flag" error instead of a deprecation notice.
    #[test]
    fn the_deprecated_kernel_memory_flag_still_parses() {
        let parsed = parse(&["--kernel-memory=10m", "ubuntu", "bash"]);
        assert_eq!(parsed.text("kernel-memory"), "10m");
        assert_eq!(parsed.args, vec!["ubuntu".to_string(), "bash".to_string()]);
    }

    /// The shorthand letters the upstream tests and workflows use. Checked on
    /// the *declaration* rather than by parsing: `-ax` on a list flag means
    /// "attach to the stream named x", which is an error, not a shorthand probe.
    #[test]
    fn the_documented_shorthands_are_present() {
        let flags = run_flag_set();
        for (long, short) in [
            ("attach", 'a'),
            ("env", 'e'),
            ("hostname", 'h'),
            ("interactive", 'i'),
            ("label", 'l'),
            ("publish", 'p'),
            ("publish-all", 'P'),
            ("tty", 't'),
            ("user", 'u'),
            ("volume", 'v'),
            ("workdir", 'w'),
            ("memory", 'm'),
            ("cpu-shares", 'c'),
        ] {
            let def = flags
                .flags()
                .iter()
                .find(|f| f.long == long)
                .unwrap_or_else(|| panic!("--{long} is missing"));
            assert_eq!(def.short, Some(short), "--{long} should carry -{short}",);
        }
    }

    /// The shorthands actually resolve to their flag when used.
    #[test]
    fn a_shorthand_reaches_its_flag() {
        let parsed = parse(&["-a", "stdin"]);
        assert_eq!(parsed.list("attach"), ["stdin".to_string()]);
        // Booleans may run together in one group.
        let parsed = parse(&["-it", "ubuntu"]);
        assert!(parsed.flag("interactive") && parsed.flag("tty"));
        assert_eq!(parsed.args, vec!["ubuntu".to_string()]);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// parse(): the flag values become a Config and a HostConfig
// ─────────────────────────────────────────────────────────────────────────────

use std::collections::{BTreeMap, BTreeSet};

use super::docker_api::{
    CgroupnsMode, Config, ContainerConfig, EndpointIpamConfig, EndpointSettings, HostConfig,
    Isolation, LogConfig, PidMode, PortBinding, Resources, UsernsMode, UtsMode,
};
use super::docker_opts::{
    parse_device, parse_logging_opts, parse_security_opts, parse_storage_opts, parse_system_paths,
    validate_device,
};
use super::docker_opts_mounts::{GpuOpts, MountOpt, NetworkAttachmentOpts, NetworkOpt};
use super::docker_opts_types::{parse_restart_policy, read_kv_env_strings, read_kv_strings};
use super::docker_specs::volume::parse_volume;
use super::docker_specs::{cdi, nat, network};
use super::pflags::ParsedFlags;

/// The result of a flag line that did not validate.
pub type ParseError = String;

/// `parse`: turn a `docker run` flag line into what the daemon is asked for.
///
/// `server_os` is the **daemon's** operating system, not the host's, because
/// the device and validation rules are the daemon's: a Windows daemon accepts a
/// device path it would reject on Linux. Passing the wrong one is a silent
/// behaviour change, not a compile error.
pub fn parse(flags: &ParsedFlags, server_os: &str) -> Result<ContainerConfig, ParseError> {
    // ── attach ───────────────────────────────────────────────────────────
    let mut attach_stdin = flags.list("attach").iter().any(|a| a == "stdin");
    let mut attach_stdout = flags.list("attach").iter().any(|a| a == "stdout");
    let mut attach_stderr = flags.list("attach").iter().any(|a| a == "stderr");
    if flags.flag("interactive") {
        attach_stdin = true;
    }
    // With no `-a` at all, stdout and stderr are attached.
    if flags.list("attach").is_empty() {
        attach_stdout = true;
        attach_stderr = true;
    }

    // ── the mac address is checked before anything else is assembled ─────
    let mac_address = flags.text("mac-address").to_string();
    if !mac_address.is_empty() {
        let trimmed = mac_address.trim();
        if parse_mac(trimmed).is_none() {
            return Err(format!("{mac_address} is not a valid mac address"));
        }
    }

    // ── swappiness is range-checked, and -1 means "unset" ────────────────
    let swappiness = flags.number("memory-swappiness", -1);
    if swappiness != -1 && !(0..=100).contains(&swappiness) {
        return Err(format!(
            "invalid value: {swappiness}. Valid memory swappiness range is 0-100"
        ));
    }

    // ── volumes: the bind mounts are split out of the volume list ────────
    let (binds, volumes) = split_binds(flags.list("volume"))?;

    // ── tmpfs: `path[:options]` ─────────────────────────────────────────
    let mut tmpfs = BTreeMap::new();
    for entry in flags.list("tmpfs") {
        let (path, options) = match entry.split_once(':') {
            Some((path, options)) => (path, options),
            None => (entry.as_str(), ""),
        };
        tmpfs.insert(path.to_string(), options.to_string());
    }

    // ── the command and the entrypoint ──────────────────────────────────
    let run_cmd = flags.args.clone();
    let entrypoint = if !flags.text("entrypoint").is_empty() {
        Some(vec![flags.text("entrypoint").to_string()])
    } else if flags.changed("entrypoint") {
        // `--entrypoint=` given empty resets the image's entrypoint, which is
        // different from omitting the flag. Hence the `Some(vec![String::new()])`.
        Some(vec![String::new()])
    } else {
        None
    };

    // ── ports ───────────────────────────────────────────────────────────
    let converted = super::docker_opts::convert_to_standard_notation(flags.list("publish"))?;
    let (exposed_from_publish, nat_bindings) = nat::parse_port_specs(&converted)?;
    // `nat` keys its map by its own `Port`; the daemon's HostConfig keys by the
    // rendered `port/proto` string, so the keys are converted here rather than
    // in either module.
    let mut port_bindings: BTreeMap<String, Vec<PortBinding>> = BTreeMap::new();
    for (port, bindings) in nat_bindings {
        port_bindings.insert(
            port.as_str().to_string(),
            bindings
                .into_iter()
                .map(|binding| PortBinding {
                    host_ip: binding.host_ip,
                    host_port: binding.host_port,
                })
                .collect(),
        );
    }

    let mut exposed_ports: BTreeSet<String> = exposed_from_publish
        .iter()
        .map(|port| port.as_str().to_string())
        .collect();

    // `--expose` also accepts a range, which expands to every port in it.
    for entry in flags.list("expose") {
        let range = network::parse_port_range(entry)
            .map_err(|err| format!("invalid range format for --expose: {err}"))?;
        for port in range.all() {
            exposed_ports.insert(port.to_string());
        }
    }

    // ── devices ─────────────────────────────────────────────────────────
    // Validation is deliberately *late*: at flag-parse time the daemon's
    // operating system is not yet known, which is why `--device` carries no
    // validator in the flag table.
    let mut device_mappings = Vec::new();
    let mut cdi_device_names = Vec::new();
    for device in flags.list("device") {
        if cdi::is_qualified_name(device) {
            cdi_device_names.push(device.clone());
            continue;
        }
        let validated = validate_device(device, server_os)?;
        device_mappings.push(parse_device(&validated, server_os)?);
    }

    // ── environment and labels ──────────────────────────────────────────
    let env = read_kv_env_strings(flags.list("env-file"), flags.list("env"))?;
    let label_pairs = read_kv_strings(flags.list("label-file"), flags.list("label"))?;
    let labels = super::docker_opts::convert_kv_strings_to_map(&label_pairs);

    // ── the namespace modes, each with its own vocabulary ───────────────
    let pid_mode = PidMode(flags.text("pid").to_string());
    if !pid_mode.valid() {
        return Err("--pid: invalid PID mode".to_string());
    }
    let uts_mode = UtsMode(flags.text("uts").to_string());
    if !uts_mode.valid() {
        return Err("--uts: invalid UTS mode".to_string());
    }
    let userns_mode = UsernsMode(flags.text("userns").to_string());
    if !userns_mode.valid() {
        return Err("--userns: invalid USER mode".to_string());
    }
    let cgroupns_mode = CgroupnsMode(flags.text("cgroupns").to_string());
    if !cgroupns_mode.valid() {
        return Err("--cgroupns: invalid CGROUP mode".to_string());
    }

    // ── policies ────────────────────────────────────────────────────────
    let restart_policy = parse_restart_policy(flags.text("restart"))?;
    let logging_opts = parse_logging_opts(flags.text("log-driver"), flags.list("log-opt"))?;
    let security_opts = parse_security_opts(flags.list("security-opt"))?;
    let (security_opts, masked_paths, readonly_paths) = parse_system_paths(&security_opts);
    let storage_opts = parse_storage_opts(flags.list("storage-opt"))?;

    // ── the health check ────────────────────────────────────────────────
    let healthcheck = parse_health(flags)?;

    // ── device requests: --gpus first, then the CDI names ───────────────
    let mut device_requests = gpus_from(flags)?;
    if !cdi_device_names.is_empty() {
        device_requests.push(super::docker_api::DeviceRequest {
            driver: "cdi".to_string(),
            device_ids: cdi_device_names,
            ..Default::default()
        });
    }

    let resources = Resources {
        cgroup_parent: flags.text("cgroup-parent").to_string(),
        memory: memory_of(flags, "memory")?,
        memory_reservation: memory_of(flags, "memory-reservation")?,
        memory_swap: memory_swap_of(flags)?,
        memory_swappiness: swappiness,
        oom_kill_disable: flags.flag("oom-kill-disable"),
        // `--cpus` is a decimal CPU count that reaches the daemon as
        // nano-CPUs, so `1.5` becomes `1500000000`. An unparseable value is
        // left to `NanoCPUs::set` in the flag layer, not re-interpreted here.
        nano_cpus: cpus_of(flags)?,
        cpu_count: flags.number("cpu-count", 0),
        cpu_percent: flags.number("cpu-percent", 0),
        cpu_shares: flags.number("cpu-shares", 0),
        cpu_period: flags.number("cpu-period", 0),
        cpuset_cpus: flags.text("cpuset-cpus").to_string(),
        cpuset_mems: flags.text("cpuset-mems").to_string(),
        cpu_quota: flags.number("cpu-quota", 0),
        cpu_realtime_period: flags.number("cpu-rt-period", 0),
        cpu_realtime_runtime: flags.number("cpu-rt-runtime", 0),
        pids_limit: flags.number("pids-limit", 0),
        blkio_weight: flags.number("blkio-weight", 0) as u32,
        device_cgroup_rules: flags.list("device-cgroup-rule").to_vec(),
        devices: device_mappings,
        device_requests,
        ..Default::default()
    };

    let mut config = Config {
        hostname: flags.text("hostname").to_string(),
        domainname: flags.text("domainname").to_string(),
        exposed_ports,
        user: flags.text("user").to_string(),
        tty: flags.flag("tty"),
        open_stdin: flags.flag("interactive"),
        attach_stdin,
        attach_stdout,
        attach_stderr,
        stdin_once: false,
        env,
        cmd: run_cmd,
        image: String::new(),
        volumes,
        entrypoint,
        working_dir: flags.text("workdir").to_string(),
        labels,
        stop_signal: flags.text("stop-signal").to_string(),
        // Only set when the flag was given, so the daemon's default applies
        // otherwise. This is the same reason `--init` is an `Option<bool>`.
        stop_timeout: flags
            .changed("stop-timeout")
            .then(|| flags.number("stop-timeout", 0)),
        healthcheck,
    };

    let auto_remove = flags.flag("rm");
    let host_config = HostConfig {
        binds,
        container_id_file: flags.text("cidfile").to_string(),
        oom_score_adj: flags.number("oom-score-adj", 0),
        auto_remove,
        privileged: flags.flag("privileged"),
        port_bindings,
        links: flags.list("link").to_vec(),
        publish_all_ports: flags.flag("publish-all"),
        dns: flags.list("dns").to_vec(),
        dns_search: flags.list("dns-search").to_vec(),
        dns_options: flags.list("dns-option").to_vec(),
        extra_hosts: flags.list("add-host").to_vec(),
        volumes_from: flags.list("volumes-from").to_vec(),
        ipc_mode: flags.text("ipc").to_string(),
        network_mode: network_opt_from(flags)?.network_mode().to_string(),
        pid_mode,
        uts_mode,
        userns_mode,
        cgroupns_mode,
        cap_add: flags.list("cap-add").to_vec(),
        cap_drop: flags.list("cap-drop").to_vec(),
        group_add: flags.list("group-add").to_vec(),
        restart_policy,
        security_opt: security_opts,
        storage_opt: storage_opts,
        readonly_rootfs: flags.flag("read-only"),
        log_config: LogConfig {
            kind: flags.text("log-driver").to_string(),
            config: logging_opts,
        },
        volume_driver: flags.text("volume-driver").to_string(),
        isolation: Isolation(flags.text("isolation").to_string()),
        shm_size: memory_of(flags, "shm-size")?,
        resources,
        tmpfs,
        sysctls: flags.map("sysctl"),
        runtime: flags.text("runtime").to_string(),
        mounts: mount_opt_from(flags)?,
        masked_paths,
        readonly_paths,
        annotations: flags.map("annotation"),
        init: flags.changed("init").then(|| flags.flag("init")),
    };

    if auto_remove && !host_config.restart_policy.is_none() {
        return Err("conflicting options: cannot specify both --restart and --rm".to_string());
    }

    // Allocating stdin in attached mode closes it on client disconnect.
    if config.open_stdin && config.attach_stdin {
        config.stdin_once = true;
    }

    let endpoints = parse_network_opts(flags)?;

    Ok(ContainerConfig {
        config,
        host_config,
        endpoints,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// the pieces `parse` is assembled from
// ─────────────────────────────────────────────────────────────────────────────

/// `--gpus` into device requests.
fn gpus_from(flags: &ParsedFlags) -> Result<Vec<super::docker_api::DeviceRequest>, ParseError> {
    let mut gpus = GpuOpts::default();
    for value in flags.list("gpus") {
        gpus.set(value)?;
    }
    Ok(gpus.value().to_vec())
}

/// `--mount` into the daemon's mount list.
fn mount_opt_from(
    flags: &ParsedFlags,
) -> Result<Vec<super::docker_opts_mounts::mount::Mount>, ParseError> {
    let mut mounts = MountOpt::default();
    for value in flags.list("mount") {
        mounts.set(value)?;
    }
    Ok(mounts.value().to_vec())
}

/// `--network` and its hidden `--net` spelling.
///
/// Both write into the same option, so order between the two spellings is
/// preserved and a network named twice is caught rather than silently
/// overwritten.
fn network_opt_from(flags: &ParsedFlags) -> Result<NetworkOpt, ParseError> {
    let mut network = NetworkOpt::default();
    for value in flags.list("network").iter().chain(flags.list("net")) {
        network.set(value)?;
    }
    Ok(network)
}

/// `--cpus` as nano-CPUs.
fn cpus_of(flags: &ParsedFlags) -> Result<i64, ParseError> {
    if !flags.changed("cpus") {
        return Ok(0);
    }
    let raw = flags.text("cpus");
    if raw.is_empty() {
        return Ok(0);
    }
    super::docker_opts_types::parse_cpus(raw)
}

/// A memory limit, in bytes.
///
/// `-1` is a *value*, not "unset": it is how `--memory` says "no limit", and
/// it reaches the daemon as `-1`.
fn memory_of(flags: &ParsedFlags, name: &str) -> Result<i64, ParseError> {
    if !flags.changed(name) {
        return Ok(0);
    }
    let raw = flags.text(name);
    if raw.is_empty() {
        return Ok(0);
    }
    super::docker_opts_types::ram_in_bytes(raw)
}

/// `--memory-swap`, where `-1` is unlimited and the default differs.
fn memory_swap_of(flags: &ParsedFlags) -> Result<i64, ParseError> {
    if !flags.changed("memory-swap") {
        return Ok(0);
    }
    let raw = flags.text("memory-swap");
    if raw.is_empty() {
        return Ok(0);
    }
    if raw == "-1" {
        return Ok(-1);
    }
    super::docker_opts_types::ram_in_bytes(raw)
}

/// The health check, or `None` when neither a check nor `--no-healthcheck`
/// was given.
///
/// `--no-healthcheck` **conflicts** with any `--health-*` option rather than
/// overriding it, so a workflow setting both is rejected instead of silently
/// losing the check.
fn parse_health(
    flags: &ParsedFlags,
) -> Result<Option<super::docker_api::HealthConfig>, ParseError> {
    use super::docker_api::HealthConfig;
    let interval = flags.duration("health-interval", 0);
    let timeout = flags.duration("health-timeout", 0);
    let start_period = flags.duration("health-start-period", 0);
    let start_interval = flags.duration("health-start-interval", 0);
    let retries = flags.number("health-retries", 0);
    let has_settings = !flags.text("health-cmd").is_empty()
        || interval != 0
        || timeout != 0
        || start_period != 0
        || retries != 0
        || start_interval != 0;

    if flags.flag("no-healthcheck") {
        if has_settings {
            return Err("--no-healthcheck conflicts with --health-* options".to_string());
        }
        return Ok(Some(HealthConfig::none()));
    }
    if !has_settings {
        return Ok(None);
    }

    for (name, value) in [
        ("--health-interval", interval),
        ("--health-timeout", timeout),
        ("--health-retries", retries),
        ("--health-start-period", start_period),
        ("--health-start-interval", start_interval),
    ] {
        if value < 0 {
            return Err(format!("{name} cannot be negative"));
        }
    }

    let test = if flags.text("health-cmd").is_empty() {
        Vec::new()
    } else {
        vec![
            "CMD-SHELL".to_string(),
            flags.text("health-cmd").to_string(),
        ]
    };
    Ok(Some(HealthConfig {
        test,
        interval,
        timeout,
        start_period,
        start_interval,
        retries,
    }))
}

/// Go's `net.ParseMAC`, for the forms a `--mac-address` may take.
///
/// Six or eight colon- or dash-separated octets, twelve bare hex digits, and
/// the four-group dot-separated IPv4 form. Rejecting a malformed address here
/// is what keeps a typo from reaching the daemon, which would answer with a
/// worse message and no indication of which flag was at fault.
fn parse_mac(value: &str) -> Option<Vec<u8>> {
    // The dot form is the IPv4-in-MAC spelling: `0011.2233.4455.6677`.
    if value.contains('.') {
        let groups: Vec<&str> = value.split('.').collect();
        if groups.len() != 4 {
            return None;
        }
        let mut out = Vec::with_capacity(6);
        for group in groups {
            if group.len() != 4 {
                return None;
            }
            out.push(hex_byte(&group[..2])?);
            out.push(hex_byte(&group[2..])?);
        }
        return Some(out);
    }
    if let Some(separator) = value.chars().find(|c| *c == ':' || *c == '-') {
        let octets: Vec<&str> = value.split(separator).collect();
        if !matches!(octets.len(), 6 | 8) {
            return None;
        }
        return octets
            .iter()
            .map(|octet| (octet.len() == 2).then(|| hex_byte(octet)).flatten())
            .collect();
    }
    // The bare twelve-digit form, which is one 48-bit address written without
    // separators.
    if value.len() == 12 {
        return (0..6).map(|i| hex_byte(&value[i * 2..i * 2 + 2])).collect();
    }
    None
}

/// Two hex digits, case-insensitive.
fn hex_byte(value: &str) -> Option<u8> {
    if !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u8::from_str_radix(value, 16).ok()
}

/// Splits `-v` specs into the bind list and the image's volume list.
///
/// A spec with a **source** is a bind and goes to `Binds`; one without is an
/// anonymous volume and stays in the image config. Removing binds from the
/// volume map is the point: a bind must not be committed to the image's config,
/// or a later `docker run` of that image would re-apply the host path.
///
/// A bind whose host part is *relative* and starts with `.` is made absolute
/// first, so the daemon receives an unambiguous path.
fn split_binds(
    volumes: &[String],
) -> Result<(Vec<String>, BTreeMap<String, serde_json::Value>), ParseError> {
    let mut binds = Vec::new();
    let mut remaining: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for spec in volumes {
        let parsed = parse_volume(spec)?;
        if parsed.source.is_empty() {
            remaining.insert(spec.clone(), serde_json::Value::Null);
            continue;
        }
        let mut bind = spec.clone();
        if parsed.mount_type == super::docker_opts_mounts::mount::TYPE_BIND {
            if let Some((host, target)) = spec.split_once(':') {
                if !host.starts_with('/') && host.starts_with('.') {
                    if let Ok(absolute) = std::path::absolute(host) {
                        bind = format!("{}:{}", absolute.display(), target);
                    }
                }
            }
        }
        binds.push(bind);
    }
    Ok((binds, remaining))
}

#[cfg(test)]
mod parse_tests {
    use super::*;
    use crate::container::docker_api::RESTART_POLICY_ON_FAILURE;

    /// Upstream's `parseRun`: the flag line, plus the image and command that
    /// `mustParse` appends.
    fn parse_run(server_os: &str, args: &[&str]) -> Result<ContainerConfig, ParseError> {
        let mut argv: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        argv.push("ubuntu".to_string());
        argv.push("bash".to_string());
        let flags = run_flag_set().parse(&argv).map_err(|err| err.to_string())?;
        parse(&flags, server_os)
    }

    /// Upstream's `mustParse` on Linux, which is where the tables below were
    /// written; two of them skip on any other host.
    fn must_parse(args: &[&str]) -> ContainerConfig {
        parse_run("linux", args).unwrap_or_else(|err| panic!("parse({args:?}) failed: {err}"))
    }

    // docker_cli_test.go: TestParseRunAttach
    #[test]
    fn attaching_follows_a_and_i() {
        let table: &[(&[&str], bool, bool, bool)] = &[
            (&[], false, true, true),
            (&["-i"], true, true, true),
            (&["-a", "stdin"], true, false, false),
            (&["-a", "stdin", "-a", "stdout"], true, true, false),
            (
                &["-a", "stdin", "-a", "stdout", "-a", "stderr"],
                true,
                true,
                true,
            ),
        ];
        for (args, stdin, stdout, stderr) in table {
            let parsed = must_parse(args);
            assert_eq!(parsed.config.attach_stdin, *stdin, "stdin for {args:?}");
            assert_eq!(parsed.config.attach_stdout, *stdout, "stdout for {args:?}");
            assert_eq!(parsed.config.attach_stderr, *stderr, "stderr for {args:?}");
        }
    }

    /// `--interactive` with an attached stdin closes stdin on disconnect; that
    /// is the one place `StdinOnce` is ever set.
    #[test]
    fn an_attached_interactive_stdin_closes_on_disconnect() {
        assert!(must_parse(&["-i"]).config.stdin_once);
        assert!(!must_parse(&["-a", "stdin"]).config.stdin_once);
        assert!(!must_parse(&[]).config.stdin_once);
    }

    // docker_cli_test.go: TestParseHostname, TestParseHostnameDomainname
    #[test]
    fn the_hostname_and_domainname_are_carried_through() {
        let parsed = must_parse(&["-h", "myhost"]);
        assert_eq!(parsed.config.hostname, "myhost");
        assert!(parsed.config.domainname.is_empty());

        let parsed = must_parse(&["--hostname", "myhost"]);
        assert_eq!(parsed.config.hostname, "myhost");

        let parsed = must_parse(&["--domainname", "mydomain"]);
        assert_eq!(parsed.config.domainname, "mydomain");

        let parsed = must_parse(&["-h", "myhost", "--domainname", "mydomain"]);
        assert_eq!(parsed.config.hostname, "myhost");
        assert_eq!(parsed.config.domainname, "mydomain");
    }

    // docker_cli_test.go: TestParseEntryPoint
    #[test]
    fn an_empty_entrypoint_resets_rather_than_leaves_the_image_alone() {
        // No flag: the image's own entrypoint applies.
        assert!(must_parse(&[]).config.entrypoint.is_none());
        // A value: an override.
        assert_eq!(
            must_parse(&["--entrypoint", "/bin/sh"]).config.entrypoint,
            Some(vec!["/bin/sh".to_string()])
        );
        // Given empty: a reset, which is neither of the two above.
        let parsed = must_parse(&["--entrypoint="]);
        assert!(parsed.config.entrypoint_is_reset());
        assert_ne!(parsed.config.entrypoint, None);
    }

    // docker_cli_test.go: TestRunFlagsParseWithMemory
    #[test]
    fn a_memory_limit_is_converted_to_bytes() {
        for (input, want) in [
            ("100M", 104_857_600_i64),
            ("2g", 2_147_483_648),
            ("512K", 524_288),
            ("1b", 1),
        ] {
            let parsed = must_parse(&["-m", input]);
            assert_eq!(
                parsed.host_config.resources.memory, want,
                "--memory {input}",
            );
        }
    }

    // docker_cli_test.go: TestParseWithMemorySwap
    #[test]
    fn a_memory_swap_limit_keeps_minus_one() {
        assert_eq!(
            must_parse(&["--memory-swap", "2g"])
                .host_config
                .resources
                .memory_swap,
            2_147_483_648
        );
        // `-1` is unlimited, and must survive as `-1` rather than becoming 0.
        assert_eq!(
            must_parse(&["--memory-swap=-1"])
                .host_config
                .resources
                .memory_swap,
            -1
        );
    }

    // docker_cli_test.go: TestRunFlagsParseShmSize
    #[test]
    fn a_shm_size_is_converted_to_bytes() {
        assert_eq!(
            must_parse(&["--shm-size=64M"]).host_config.shm_size,
            67_108_864
        );
        assert_eq!(
            must_parse(&["--shm-size=2g"]).host_config.shm_size,
            2_147_483_648
        );
        // A malformed size is an error, not a silent zero.
        assert!(parse_run("linux", &["--shm-size=abc"]).is_err());
    }

    // docker_cli_test.go: TestParseWithExpose
    #[test]
    fn exposed_ports_include_a_range() {
        let parsed = must_parse(&["--expose", "80"]);
        assert!(
            parsed.config.exposed_ports.contains("80/tcp"),
            "80 should be exposed"
        );

        // A range expands to every port in it.
        let parsed = must_parse(&["--expose", "8000-8005"]);
        for port in 8000..=8005 {
            assert!(
                parsed.config.exposed_ports.contains(&format!("{port}/tcp")),
                "{port} should be exposed",
            );
        }
        // With a protocol.
        let parsed = must_parse(&["--expose", "80/udp"]);
        assert!(parsed.config.exposed_ports.contains("80/udp"));

        // A malformed range is an error naming the flag.
        let err = parse_run("linux", &["--expose", "invalid"]).unwrap_err();
        assert!(
            err.contains("invalid range format for --expose"),
            "got {err:?}",
        );
    }

    // docker_cli_test.go: TestParseModes
    #[test]
    fn each_namespace_accepts_only_its_own_values() {
        // pid takes a container, uts does not.
        assert!(must_parse(&["--pid=host"]).host_config.pid_mode.valid());
        assert!(must_parse(&["--pid=container:abc"])
            .host_config
            .pid_mode
            .valid());
        assert!(must_parse(&["--uts=host"]).host_config.uts_mode.valid());

        assert_eq!(
            parse_run("linux", &["--pid=container:"]).unwrap_err(),
            "--pid: invalid PID mode"
        );
        assert_eq!(
            parse_run("linux", &["--uts=container:"]).unwrap_err(),
            "--uts: invalid UTS mode"
        );
        assert_eq!(
            parse_run("linux", &["--uts=container:abc"]).unwrap_err(),
            "--uts: invalid UTS mode",
        );
    }

    // docker_cli_test.go: TestParseRestartPolicy
    #[test]
    fn a_restart_policy_is_parsed_with_its_retry_count() {
        let table: &[(&str, &str, i64)] = &[
            ("", "", 0),
            ("no", "no", 0),
            ("always", "always", 0),
            ("always:1", "always", 1),
            ("on-failure:1", RESTART_POLICY_ON_FAILURE, 1),
            ("unless-stopped", "unless-stopped", 0),
        ];
        for (input, name, retries) in table {
            let policy = must_parse(&["--restart", input]).host_config.restart_policy;
            assert_eq!(policy.name, *name, "name for --restart={input:?}");
            assert_eq!(
                policy.maximum_retry_count, *retries,
                "retries for {input:?}"
            );
        }

        for (input, message) in [
            (
                ":1",
                "invalid restart policy format: no policy provided before colon",
            ),
            (
                "always:2:3",
                "invalid restart policy format: maximum retry count must be an integer",
            ),
            (
                "on-failure:invalid",
                "invalid restart policy format: maximum retry count must be an integer",
            ),
        ] {
            assert_eq!(
                parse_run("linux", &[&format!("--restart={input}")]).unwrap_err(),
                message,
                "for --restart={input:?}",
            );
        }
    }

    // docker_cli_test.go: TestParseRestartPolicyAutoRemove
    #[test]
    fn rm_and_restart_conflict() {
        assert_eq!(
            parse_run("linux", &["--rm", "--restart=always"]).unwrap_err(),
            "conflicting options: cannot specify both --restart and --rm"
        );
        // `--rm` with the *default* policy is fine, because the empty name
        // counts as disabled.
        assert!(must_parse(&["--rm"]).host_config.auto_remove);
    }

    // docker_cli_test.go: TestParseHealth
    #[test]
    fn a_healthcheck_is_built_from_its_flags() {
        let parsed = must_parse(&["--health-cmd", "curl -f http://localhost/"]);
        let health = parsed.config.healthcheck.expect("a health check");
        assert_eq!(
            health.test,
            vec![
                "CMD-SHELL".to_string(),
                "curl -f http://localhost/".to_string()
            ]
        );
        assert_eq!(health.interval, 0);

        let parsed = must_parse(&["--health-interval", "5s", "--health-retries", "3"]);
        let health = parsed.config.healthcheck.expect("a health check");
        assert_eq!(health.interval, 5_000_000_000, "5s in nanoseconds");
        assert_eq!(health.retries, 3);

        // No health flags at all means no health check, so the image's own
        // HEALTHCHECK applies.
        assert!(must_parse(&[]).config.healthcheck.is_none());

        // `--no-healthcheck` is the NONE probe.
        let health = must_parse(&["--no-healthcheck"])
            .config
            .healthcheck
            .expect("a health check");
        assert_eq!(health.test, vec!["NONE".to_string()]);
    }

    /// `--no-healthcheck` conflicts with a check rather than overriding it.
    #[test]
    fn no_healthcheck_conflicts_with_health_flags() {
        assert_eq!(
            parse_run("linux", &["--no-healthcheck", "--health-cmd", "true"]).unwrap_err(),
            "--no-healthcheck conflicts with --health-* options"
        );
    }

    // docker_cli_test.go: TestParseWithMacAddress
    #[test]
    fn a_mac_address_is_validated() {
        assert!(parse_run("linux", &["--mac-address", "92:d0:c6:0a:29:33"]).is_ok());
        assert!(parse_run("linux", &["--mac-address", "92-d0-c6-0a-29-33"]).is_ok());
        assert!(parse_run("linux", &["--mac-address", "92d0c60a2933"]).is_ok());
        let err = parse_run("linux", &["--mac-address", "92:d0:c6:0a:29"]).unwrap_err();
        assert!(err.contains("is not a valid mac address"), "got {err:?}");
    }

    /// The swappiness range is checked, and `-1` means "leave it to the
    /// daemon" rather than being rejected as out of range.
    #[test]
    fn a_swappiness_outside_the_range_is_rejected() {
        assert!(parse_run("linux", &["--memory-swappiness=-1"]).is_ok());
        assert!(parse_run("linux", &["--memory-swappiness=0"]).is_ok());
        assert!(parse_run("linux", &["--memory-swappiness=100"]).is_ok());
        assert_eq!(
            parse_run("linux", &["--memory-swappiness=101"]).unwrap_err(),
            "invalid value: 101. Valid memory swappiness range is 0-100"
        );
    }

    /// `--stop-timeout` stays absent unless the flag was given, so the daemon's
    /// own default applies otherwise.
    #[test]
    fn a_stop_timeout_is_only_set_when_asked_for() {
        assert_eq!(must_parse(&[]).config.stop_timeout, None);
        assert_eq!(
            must_parse(&["--stop-timeout=10"]).config.stop_timeout,
            Some(10)
        );
    }

    /// `--init` likewise, and `--init=false` is still "changed".
    #[test]
    fn init_is_only_set_when_asked_for() {
        assert_eq!(must_parse(&[]).host_config.init, None);
        assert_eq!(must_parse(&["--init"]).host_config.init, Some(true));
        assert_eq!(must_parse(&["--init=false"]).host_config.init, Some(false));
    }

    // docker_cli_test.go: TestParseWithVolumes
    #[test]
    fn a_volume_with_a_source_becomes_a_bind() {
        let parsed = must_parse(&["-v", "/tmp:/tmp"]);
        assert_eq!(parsed.host_config.binds, vec!["/tmp:/tmp".to_string()]);
        assert!(
            parsed.config.volumes.is_empty(),
            "a bind must not be committed to the image config",
        );
    }

    /// An anonymous volume has no source, so it stays in the image's config
    /// and does not become a bind.
    #[test]
    fn an_anonymous_volume_stays_in_the_image_config() {
        let parsed = must_parse(&["-v", "/data"]);
        assert!(parsed.host_config.binds.is_empty());
        assert!(
            parsed.config.volumes.contains_key("/data"),
            "an anonymous volume belongs to the image config",
        );
    }

    /// `--tmpfs path[:options]` splits at the first colon.
    #[test]
    fn a_tmpfs_is_split_into_path_and_options() {
        let parsed = must_parse(&["--tmpfs", "/tmp:size=1m"]);
        assert_eq!(
            parsed.host_config.tmpfs.get("/tmp").map(String::as_str),
            Some("size=1m")
        );

        let parsed = must_parse(&["--tmpfs", "/tmp"]);
        assert_eq!(
            parsed.host_config.tmpfs.get("/tmp").map(String::as_str),
            Some("")
        );
    }

    // docker_cli_test.go: TestParseEnvfileVariables
    #[test]
    fn environment_variables_come_from_flags_and_files() {
        let parsed = must_parse(&["-e", "FOO=bar", "-e", "BAZ=qux"]);
        // Upstream builds this from a Go map, so the order is not observable
        // and asserting one would only pin a Rust implementation detail.
        assert!(parsed.config.env.contains(&"FOO=bar".to_string()));
        assert!(parsed.config.env.contains(&"BAZ=qux".to_string()));

        let dir = tempfile::tempdir().expect("a temporary directory");
        let file = dir.path().join("env");
        std::fs::write(&file, "BAR=baz\nQUX=1\n").expect("written");
        let path = file.to_string_lossy().to_string();
        let parsed = parse_run("linux", &["--env-file", &path]).expect("parsed");
        assert!(parsed.config.env.contains(&"BAR=baz".to_string()));
        assert!(parsed.config.env.contains(&"QUX=1".to_string()));
    }

    // docker_cli_test.go: TestParseLabelfileVariables
    #[test]
    fn labels_come_from_flags_and_files() {
        let parsed = must_parse(&["-l", "foo=bar", "-l", "baz=qux"]);
        assert_eq!(
            parsed.config.labels.get("foo").map(String::as_str),
            Some("bar")
        );
        assert_eq!(
            parsed.config.labels.get("baz").map(String::as_str),
            Some("qux")
        );

        let dir = tempfile::tempdir().expect("a temporary directory");
        let file = dir.path().join("labels");
        std::fs::write(&file, "one=1\ntwo=2\n").expect("written");
        let path = file.to_string_lossy().to_string();
        let parsed = parse_run("linux", &["--label-file", &path]).expect("parsed");
        assert_eq!(
            parsed.config.labels.get("one").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            parsed.config.labels.get("two").map(String::as_str),
            Some("2")
        );
    }

    // docker_cli_test.go: TestParseRunLinks
    #[test]
    fn links_are_carried_into_the_host_config() {
        let parsed = must_parse(&["--link", "db:database"]);
        assert_eq!(parsed.host_config.links, vec!["db:database".to_string()]);
    }

    /// `--network` with nothing else yields the daemon's own name for the
    /// default bridge.
    #[test]
    fn an_unset_network_is_the_default() {
        assert_eq!(must_parse(&[]).host_config.network_mode, "default");
        assert_eq!(
            must_parse(&["--network", "net1"]).host_config.network_mode,
            "net1"
        );
        // The hidden `--net` spelling reaches the same field.
        assert_eq!(
            must_parse(&["--net", "net1"]).host_config.network_mode,
            "net1"
        );
    }

    /// `--gpus all` becomes a device request.
    #[test]
    fn a_gpu_request_becomes_a_device_request() {
        let parsed = must_parse(&["--gpus", "all"]);
        let requests = &parsed.host_config.resources.device_requests;
        assert_eq!(requests.len(), 1, "one request");
        // `all` is a *count*, not a device name: the request asks for every
        // device and therefore names none of them.
        assert_eq!(requests[0].count, -1, "all devices");
        assert!(requests[0].device_ids.is_empty(), "no device is named");
        assert_eq!(requests[0].capabilities, vec![vec!["gpu".to_string()]]);
    }

    // docker_cli_test.go: TestParseNetworkConfig, the conflict table
    #[test]
    fn network_option_conflicts_are_reported() {
        let cases: &[(&[&str], &str)] = &[
            (
                &["--network", "duplicate", "--network", "name=duplicate"],
                r#"network "duplicate" is specified multiple times"#,
            ),
            (
                &["--network", "name=net1,alias=web1", "--network-alias", "web1"],
                "conflicting options: cannot specify both --network-alias and per-network alias",
            ),
            (
                &[
                    "--network",
                    "name=net1,ip=172.20.88.22,ip6=2001:db8::8822",
                    "--ip",
                    "172.20.88.22",
                ],
                "conflicting options: cannot specify both --ip and per-network IPv4 address",
            ),
            (
                &[
                    "--network",
                    "name=net1,ip=172.20.88.22,ip6=2001:db8::8822",
                    "--ip6",
                    "2001:db8::8822",
                ],
                "conflicting options: cannot specify both --ip6 and per-network IPv6 address",
            ),
            (
                &["--network", "name=host", "--network", "net1"],
                "conflicting options: cannot attach both user-defined and non-user-defined network-modes",
            ),
            (
                &[
                    "--network",
                    "name=net1,link-local-ip=169.254.169.254",
                    "--link-local-ip",
                    "169.254.10.8",
                ],
                "conflicting options: cannot specify both --link-local-ip and per-network link-local IP addresses",
            ),
            (
                &[
                    "--network",
                    "name=net1,mac-address=02:32:1c:23:00:04",
                    "--mac-address",
                    "02:32:1c:23:00:04",
                ],
                "conflicting options: cannot specify both --mac-address and per-network MAC address",
            ),
            (
                &["--network", "name=net1,mac-address=foobar"],
                "foobar is not a valid mac address",
            ),
        ];
        for (args, want) in cases {
            assert_eq!(parse_run("linux", args).unwrap_err(), *want, "for {args:?}",);
        }
    }

    // docker_cli_test.go: TestParseNetworkConfig, the accepted shapes
    #[test]
    fn a_lone_network_with_nothing_configured_is_left_to_the_daemon() {
        // Both the legacy and the advanced spelling name the same network, and
        // neither produces an endpoint entry: the daemon creates the default.
        for args in [vec!["--network", "net1"], vec!["--network", "name=net1"]] {
            let parsed = must_parse(&args);
            assert_eq!(parsed.host_config.network_mode, "net1", "for {args:?}");
            assert!(
                parsed.endpoints.is_empty(),
                "an unconfigured lone network leaves no endpoint, for {args:?}",
            );
        }
    }

    /// The legacy flags are folded into the one network, in the order given.
    #[test]
    fn the_legacy_flags_attach_to_the_first_network() {
        let parsed = must_parse(&[
            "--ip",
            "172.20.88.22",
            "--ip6",
            "2001:db8::8822",
            "--link",
            "foo:bar",
            "--link",
            "bar:baz",
            "--link-local-ip",
            "169.254.2.2",
            "--link-local-ip",
            "fe80::169:254:2:2",
            "--network",
            "name=net1",
            "--network-alias",
            "web1",
            "--network-alias",
            "web2",
        ]);
        assert_eq!(parsed.host_config.network_mode, "net1");

        let endpoint = parsed
            .endpoints
            .get("net1")
            .expect("an endpoint for net1")
            .as_ref()
            .expect("a configured endpoint");
        assert_eq!(
            endpoint.aliases,
            vec!["web1".to_string(), "web2".to_string()]
        );
        assert_eq!(
            endpoint.links,
            vec!["foo:bar".to_string(), "bar:baz".to_string()]
        );

        let ipam = endpoint.ipam_config.as_ref().expect("addresses were given");
        assert_eq!(ipam.ipv4_address, Some("172.20.88.22".parse().unwrap()));
        assert_eq!(ipam.ipv6_address, Some("2001:db8::8822".parse().unwrap()));
        let link_locals: Vec<std::net::IpAddr> = vec![
            "169.254.2.2".parse().unwrap(),
            "fe80::169:254:2:2".parse().unwrap(),
        ];
        assert_eq!(ipam.link_local_ips, link_locals);
    }

    /// A per-network mac address is reported on the endpoint, and it is checked
    /// — a malformed one names itself.
    #[test]
    fn a_per_network_mac_address_lands_on_the_endpoint() {
        let parsed = must_parse(&["--network=name=net1,mac-address=52:0f:f3:dc:50:10"]);
        let endpoint = parsed
            .endpoints
            .get("net1")
            .expect("an endpoint for net1")
            .as_ref()
            .expect("a configured endpoint");
        assert_eq!(endpoint.mac_address, "52:0f:f3:dc:50:10");
    }

    /// Aliases and links have nowhere to go on a network the user did not
    /// create, so they are rejected rather than dropped.
    #[test]
    fn aliases_and_links_need_a_user_defined_network() {
        assert_eq!(
            parse_run("linux", &["--network=name=host,alias=web"]).unwrap_err(),
            "network-scoped aliases are only supported for user-defined networks"
        );
    }

    /// The command and the image are the positional arguments, in order.
    #[test]
    fn the_command_is_the_positional_arguments() {
        assert_eq!(must_parse(&[]).config.cmd, vec!["ubuntu", "bash"]);
        assert_eq!(
            must_parse(&["-a", "stdin"]).config.cmd,
            vec!["ubuntu", "bash"]
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// the network endpoints
// ─────────────────────────────────────────────────────────────────────────────

/// `parseNetworkOpts`: one endpoint per network, plus the legacy flags folded
/// into the first one.
///
/// Three rules make this more than a map construction:
///
/// 1. **With no `--network` at all**, a single `default` endpoint is produced.
/// 2. **The legacy flags apply only to the first network.** `--network-alias`,
///    `--link`, `--ip`, `--ip6`, `--mac-address` and `--link-local-ip` were
///    written before `--network` accepted inline options, so they attach to the
///    first network only — which is the only one there was when they were
///    written.
/// 3. **A lone network with nothing configured on it is omitted entirely**, so
///    the daemon creates its own default rather than being told "an empty
///    endpoint, please".
fn parse_network_opts(
    flags: &ParsedFlags,
) -> Result<BTreeMap<String, Option<EndpointSettings>>, ParseError> {
    let network = network_opt_from(flags)?;
    let attachments: Vec<NetworkAttachmentOpts> = network.value().to_vec();
    let mut endpoints: BTreeMap<String, Option<EndpointSettings>> = BTreeMap::new();
    let mut has_user_defined = false;
    let mut has_non_user_defined = false;

    if attachments.is_empty() {
        let mut attachment = NetworkAttachmentOpts {
            target: "default".to_string(),
            ..Default::default()
        };
        apply_container_options(&mut attachment, flags)?;
        endpoints.insert(
            "default".to_string(),
            parse_network_attachment_opt(&attachment)?,
        );
    }

    for (index, attachment) in attachments.iter().enumerate() {
        let mode = super::docker_api::NetworkMode(attachment.target.clone());
        if mode.is_user_defined() {
            has_user_defined = true;
        } else {
            has_non_user_defined = true;
        }

        let mut attachment = attachment.clone();
        if index == 0 {
            apply_container_options(&mut attachment, flags)?;
        }
        let endpoint = parse_network_attachment_opt(&attachment)?;
        if endpoints.contains_key(&attachment.target) {
            return Err(format!(
                "network {:?} is specified multiple times",
                attachment.target
            ));
        }
        // The lone-default-network case: an endpoint with nothing set is
        // dropped so the daemon fills it in.
        if index == 0 && attachments.len() == 1 && endpoint.is_none() {
            continue;
        }
        endpoints.insert(attachment.target.clone(), endpoint);
    }

    if has_user_defined && has_non_user_defined {
        return Err(
            "conflicting options: cannot attach both user-defined and non-user-defined network-modes"
                .to_string(),
        );
    }
    Ok(endpoints)
}

/// `applyContainerOptions`: fold the legacy per-container flags into one
/// attachment.
///
/// Each legacy flag **conflicts** with its inline equivalent rather than
/// merging with it — `--link` and a per-network `link=` are two ways to say the
/// same thing, and silently preferring one would hide a mistake.
///
/// One asymmetry: `--link` becomes a *link* only on a user-defined network. On
/// the default bridge the same flag is a legacy link, and on a non-user-defined
/// network it is simply not copied.
fn apply_container_options(
    attachment: &mut NetworkAttachmentOpts,
    flags: &ParsedFlags,
) -> Result<(), ParseError> {
    if !attachment.aliases.is_empty() && !flags.list("network-alias").is_empty() {
        return Err(
            "conflicting options: cannot specify both --network-alias and per-network alias"
                .to_string(),
        );
    }
    if !attachment.links.is_empty() && !flags.list("link").is_empty() {
        return Err(
            "conflicting options: cannot specify both --link and per-network links".to_string(),
        );
    }
    if attachment.ipv4_address.is_some() && flags.changed("ip") {
        return Err(
            "conflicting options: cannot specify both --ip and per-network IPv4 address"
                .to_string(),
        );
    }
    if attachment.ipv6_address.is_some() && flags.changed("ip6") {
        return Err(
            "conflicting options: cannot specify both --ip6 and per-network IPv6 address"
                .to_string(),
        );
    }
    if !attachment.mac_address.is_empty() && !flags.text("mac-address").is_empty() {
        return Err(
            "conflicting options: cannot specify both --mac-address and per-network MAC address"
                .to_string(),
        );
    }
    if !attachment.link_local_ips.is_empty() && !flags.list("link-local-ip").is_empty() {
        return Err(
            "conflicting options: cannot specify both --link-local-ip and per-network link-local IP addresses"
                .to_string(),
        );
    }

    // Both of these are assigned **conditionally**. Assigning unconditionally
    // would wipe the per-network values the inline syntax already put there,
    // and `--network name=host,alias=web` would silently lose its alias.
    if !flags.list("network-alias").is_empty() {
        attachment.aliases = flags.list("network-alias").to_vec();
    }
    // `--link` is an endpoint option on a user-defined network and a legacy
    // link elsewhere, so it is only copied in the first case.
    if super::docker_api::NetworkMode(attachment.target.clone()).is_user_defined()
        && !flags.list("link").is_empty()
    {
        attachment.links = flags.list("link").to_vec();
    }
    if flags.changed("ip") {
        attachment.ipv4_address = flags.text("ip").trim().parse().ok();
    }
    if flags.changed("ip6") {
        attachment.ipv6_address = flags.text("ip6").trim().parse().ok();
    }
    if !flags.text("mac-address").is_empty() {
        attachment.mac_address = flags.text("mac-address").to_string();
    }
    if !flags.list("link-local-ip").is_empty() {
        attachment.link_local_ips = flags
            .list("link-local-ip")
            .iter()
            .filter_map(|value| value.trim().parse().ok())
            .collect();
    }
    Ok(())
}

/// `parseNetworkAttachmentOpt`: one network's settings, or `None` when it has
/// none to report.
///
/// `None` is not an error — it is the "nothing configured" answer that
/// `parse_network_opts` then drops for a lone network. Aliases and links are
/// rejected on a network the user did not create, because the daemon has
/// nowhere to put them.
fn parse_network_attachment_opt(
    attachment: &NetworkAttachmentOpts,
) -> Result<Option<EndpointSettings>, ParseError> {
    if attachment.target.trim().is_empty() {
        return Err("no name set for network".to_string());
    }
    let user_defined = super::docker_api::NetworkMode(attachment.target.clone()).is_user_defined();
    if !user_defined {
        if !attachment.aliases.is_empty() {
            return Err(
                "network-scoped aliases are only supported for user-defined networks".to_string(),
            );
        }
        if !attachment.links.is_empty() {
            return Err("links are only supported for user-defined networks".to_string());
        }
    }

    let settings = EndpointSettings {
        aliases: attachment.aliases.clone(),
        driver_opts: attachment
            .driver_opts
            .as_ref()
            .filter(|opts| !opts.is_empty())
            .cloned(),
        links: attachment.links.clone(),
        gw_priority: attachment.gw_priority,
        ipam_config: (attachment.ipv4_address.is_some()
            || attachment.ipv6_address.is_some()
            || !attachment.link_local_ips.is_empty())
        .then(|| EndpointIpamConfig {
            ipv4_address: attachment.ipv4_address,
            ipv6_address: attachment.ipv6_address,
            link_local_ips: attachment.link_local_ips.clone(),
        }),
        mac_address: String::new(),
    };

    // A settings value that differs from Go's zero value at all counts as
    // "configured"; otherwise the endpoint is reported as absent.
    let zero = EndpointSettings::default();
    let configured = settings.aliases != zero.aliases
        || settings.driver_opts != zero.driver_opts
        || settings.links != zero.links
        || settings.gw_priority != zero.gw_priority
        || settings.ipam_config != zero.ipam_config
        || !attachment.mac_address.is_empty();

    if !configured {
        return Ok(None);
    }

    let mut settings = settings;
    if !attachment.mac_address.is_empty() {
        let trimmed = attachment.mac_address.trim();
        if parse_mac(trimmed).is_none() {
            return Err(format!(
                "{} is not a valid mac address",
                attachment.mac_address
            ));
        }
        settings.mac_address = trimmed.to_string();
    }
    Ok(Some(settings))
}
