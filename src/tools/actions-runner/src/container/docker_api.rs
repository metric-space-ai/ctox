//! The `container` API types `parse()` produces: the namespaces, policies and
//! health checks that end up in a `HostConfig`.
//!
//! This is `github.com/moby/moby/api/types/container`, and it is ported rather
//! than imported for two reasons. `bollard` exposes the same wire shapes under
//! its own names, so a translation would exist anyway — and putting it in one
//! place makes the *validation* rules visible, which is where the interesting
//! behaviour is.
//!
//! # The mode rules are not one rule each
//!
//! Every namespace has a `Valid()`, and no two of them accept the same set:
//!
//! | mode | accepts |
//! |---|---|
//! | `pid` | `""`, `host`, `container:<id>` **with a non-empty id** |
//! | `uts` | `""`, `host` — no container form at all |
//! | `userns` | `""`, `host` |
//! | `cgroupns` | `""`, `private`, `host` |
//! | `ipc` | `""`, `none`, `private`, `host`, `shareable`, `container:<id>` |
//!
//! `container:` with nothing after it is a **`PidMode` and `IpcMode` error but
//! not an `IpcMode`-independent one** — the id is parsed out and then required
//! to be non-empty, which is why `validContainer` and `IsContainer` are two
//! different functions upstream. Getting `uts` wrong is the easy mistake: it
//! has no `container:` form at all, so `--uts=container:foo` is invalid even
//! though `--pid=container:foo` is fine.
//!
//! # `IsNone` treats the empty name as disabled
//!
//! `RestartPolicy.IsNone()` is `Name == "no" || Name == ""`. That is what makes
//! `--rm` with a default restart policy legal: the *default* is the empty name,
//! and the default is "not a restart policy".
//!
//! # These types are the wire format
//!
//! `Config` and `HostConfig` are what the daemon is handed in the body of
//! `/containers/create`, so they derive `Serialize` under **Docker's own field
//! names** — taken from the struct tags in
//! `github.com/moby/moby/api/types/container`, not from the Rust identifiers.
//! The two disagree often enough to matter: `NetworkMode` → `NetworkMode`,
//! `dns` → `Dns`, `cpu_shares` → `CpuShares`, `nano_cpus` → `NanoCpus`,
//! `uts_mode` → `UTSMode`.
//!
//! Go's `,omitempty` is reproduced with `skip_serializing_if` on the same
//! fields. That is not cosmetic: `Resources.MemorySwap` is `omitempty` and
//! `Config.StopSignal` is too, and a field sent as a zero where the original
//! omitted it is a different request to the daemon.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Serialize;

/// `container:<id>`: the name or id of another container to share with.
///
/// Shared by every mode that accepts a container, and the reason two
/// functions exist upstream — one that recognises the prefix, and one that also
/// demands a non-empty id.
fn container_id(value: &str) -> Option<&str> {
    value
        .split_once(':')
        .filter(|(key, _)| *key == "container")
        .map(|(_, id)| id)
}

/// `validContainer`: the prefix is present *and* names something.
fn valid_container(value: &str) -> bool {
    container_id(value).is_some_and(|id| !id.is_empty())
}

/// `PidMode`: `""`, `host`, or `container:<id>`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(transparent)]
pub struct PidMode(pub String);

impl PidMode {
    /// The container named by a `container:<id>` mode.
    pub fn container(&self) -> Option<&str> {
        container_id(&self.0)
    }

    /// Whether the host's pid namespace is used.
    pub fn is_host(&self) -> bool {
        self.0 == "host"
    }

    /// `Valid`.
    pub fn valid(&self) -> bool {
        self.0.is_empty() || self.is_host() || valid_container(&self.0)
    }
}

/// `UTSMode`: `""` or `host`. There is no container form.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(transparent)]
pub struct UtsMode(pub String);

impl UtsMode {
    /// Whether the host's UTS namespace is used.
    pub fn is_host(&self) -> bool {
        self.0 == "host"
    }

    /// `Valid`.
    pub fn valid(&self) -> bool {
        self.0.is_empty() || self.is_host()
    }
}

/// `UsernsMode`: `""` or `host`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(transparent)]
pub struct UsernsMode(pub String);

impl UsernsMode {
    /// Whether the host's user namespace is used.
    pub fn is_host(&self) -> bool {
        self.0 == "host"
    }

    /// Whether the container gets its own user namespace — which is every value
    /// that is *not* `host`.
    pub fn is_private(&self) -> bool {
        !self.is_host()
    }

    /// `Valid`.
    pub fn valid(&self) -> bool {
        self.0.is_empty() || self.is_host()
    }
}

/// `CgroupnsMode`: `""`, `private` or `host`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(transparent)]
pub struct CgroupnsMode(pub String);

impl CgroupnsMode {
    /// Whether the container gets its own cgroup namespace.
    pub fn is_private(&self) -> bool {
        self.0 == "private"
    }

    /// Whether the host's cgroup namespace is used.
    pub fn is_host(&self) -> bool {
        self.0 == "host"
    }

    /// `Valid`.
    pub fn valid(&self) -> bool {
        self.0.is_empty() || self.is_private() || self.is_host()
    }
}

/// `IpcMode`: `""`, `none`, `private`, `host`, `shareable` or `container:<id>`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(transparent)]
pub struct IpcMode(pub String);

impl IpcMode {
    /// Whether IPC is switched off entirely.
    pub fn is_none(&self) -> bool {
        self.0 == "none"
    }

    /// Whether the host's IPC namespace is used.
    pub fn is_host(&self) -> bool {
        self.0 == "host"
    }

    /// Whether the container gets its own IPC namespace.
    pub fn is_private(&self) -> bool {
        self.0 == "private"
    }

    /// Whether the namespace can be shared with another container.
    pub fn is_shareable(&self) -> bool {
        self.0 == "shareable"
    }

    /// The container named by a `container:<id>` mode.
    pub fn container(&self) -> Option<&str> {
        container_id(&self.0)
    }

    /// Whether the prefix is present, whatever follows it. Note this differs
    /// from [`IpcMode::valid`], which additionally requires a non-empty id.
    pub fn is_container(&self) -> bool {
        container_id(&self.0).is_some()
    }

    /// Whether nothing was specified.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `Valid`.
    pub fn valid(&self) -> bool {
        self.is_empty()
            || self.is_none()
            || self.is_private()
            || self.is_host()
            || self.is_shareable()
            || self.is_container()
    }
}

/// `NetworkMode`: `default`, `none`, `host`, `bridge`, or a network name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(transparent)]
pub struct NetworkMode(pub String);

impl NetworkMode {
    /// Whether the container has no network stack.
    pub fn is_none(&self) -> bool {
        self.0 == "none"
    }

    /// Whether the default network stack is used.
    pub fn is_default(&self) -> bool {
        self.0 == "default"
    }

    /// Whether the host's network stack is used.
    pub fn is_host(&self) -> bool {
        self.0 == "host"
    }

    /// Whether Docker's default bridge is used.
    pub fn is_bridge(&self) -> bool {
        self.0 == "bridge"
    }

    /// Whether the mode is `container:<id>`, sharing another container's stack.
    pub fn is_container(&self) -> bool {
        self.0
            .split_once(':')
            .is_some_and(|(key, _)| key == "container")
    }

    /// Whether the container gets its own private network stack.
    pub fn is_private(&self) -> bool {
        !self.is_host() && !self.is_container()
    }

    /// `IsUserDefined`: a network the user created, as opposed to one of the
    /// daemon's built-ins.
    ///
    /// **The answer is platform-dependent, and so is this.** moby has two
    /// definitions — `hostconfig_unix.go` and `hostconfig_windows.go` — and
    /// they differ in whether `host` counts as user-defined. It does not on
    /// Unix and it does on Windows, so the same `--network host` is classified
    /// differently on the two build computers. The two branches below are that
    /// difference, kept because "the same workflow behaves differently on a
    /// Linux and a Windows CI computer" is exactly the kind of surprise this
    /// port exists to prevent.
    pub fn is_user_defined(&self) -> bool {
        #[cfg(windows)]
        {
            !self.is_default() && !self.is_none() && !self.is_bridge() && !self.is_container()
        }
        #[cfg(not(windows))]
        {
            !self.is_default()
                && !self.is_bridge()
                && !self.is_host()
                && !self.is_none()
                && !self.is_container()
        }
    }

    /// `UserDefined`: the network name, or empty when the mode is built-in.
    pub fn user_defined(&self) -> &str {
        if self.is_user_defined() {
            &self.0
        } else {
            ""
        }
    }
}

/// `Isolation`: the container isolation technology.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(transparent)]
pub struct Isolation(pub String);

impl Isolation {
    /// The daemon's default isolation, compared case-insensitively — upstream
    /// leaves a TODO about making this strict, so it is not.
    pub fn is_default(&self) -> bool {
        self.0.to_lowercase() == "default"
    }
}

/// The names `RestartPolicy` can carry.
pub const RESTART_POLICY_DISABLED: &str = "no";
pub const RESTART_POLICY_ALWAYS: &str = "always";
pub const RESTART_POLICY_ON_FAILURE: &str = "on-failure";
pub const RESTART_POLICY_UNLESS_STOPPED: &str = "unless-stopped";

/// `RestartPolicy`: a name and, for `on-failure`, a retry limit.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct RestartPolicy {
    /// One of the `RESTART_POLICY_*` names.
    #[serde(rename = "Name")]
    pub name: String,
    /// How many times to retry, meaningful only for `on-failure`.
    #[serde(rename = "MaximumRetryCount")]
    pub maximum_retry_count: i64,
}

impl RestartPolicy {
    /// `IsNone`: the policy is disabled, **or was never set**.
    ///
    /// The empty name counts as disabled, which is what lets `--rm` combine
    /// with the *default* restart policy without the "cannot specify both"
    /// conflict firing.
    pub fn is_none(&self) -> bool {
        self.name.is_empty() || self.name == RESTART_POLICY_DISABLED
    }
}

/// A resource limit for one device or path, as `--ulimit` produces.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Ulimit {
    /// The resource name, e.g. `nofile`.
    #[serde(rename = "Name")]
    pub name: String,
    /// The soft limit.
    #[serde(rename = "Soft")]
    pub soft: i64,
    /// The hard limit.
    #[serde(rename = "Hard")]
    pub hard: i64,
}

/// A CDI device request, or a `--gpus` request.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct DeviceRequest {
    /// The driver that resolves the device, e.g. `cdi` or `nvidia`.
    #[serde(rename = "Driver")]
    pub driver: String,
    /// The device names, and optionally counts and capabilities.
    #[serde(rename = "DeviceIDs")]
    pub device_ids: Vec<String>,
    /// Required capabilities, `[]` for an empty request.
    #[serde(rename = "Capabilities")]
    pub capabilities: Vec<Vec<String>>,
    /// Whether all devices matching the request are taken.
    #[serde(rename = "Count")]
    pub count: i64,
    /// An opaque driver-specific option blob.
    #[serde(rename = "Options")]
    pub options: BTreeMap<String, String>,
}

/// `HealthConfig`: the container's `HEALTHCHECK`, or `NONE`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct HealthConfig {
    /// `["CMD-SHELL", cmd]`, or `["NONE"]` when the check is disabled.
    #[serde(rename = "Test")]
    pub test: Vec<String>,
    /// Nanoseconds between checks.
    #[serde(rename = "Interval")]
    pub interval: i64,
    /// Nanoseconds allowed for one check.
    #[serde(rename = "Timeout")]
    pub timeout: i64,
    /// Nanoseconds before the first check, during which failures are ignored.
    #[serde(rename = "StartPeriod")]
    pub start_period: i64,
    /// Nanoseconds between checks during the start period.
    #[serde(rename = "StartInterval")]
    pub start_interval: i64,
    /// Consecutive failures before the container is reported unhealthy.
    #[serde(rename = "Retries")]
    pub retries: i64,
}

impl HealthConfig {
    /// The config produced by `--no-healthcheck`.
    pub fn none() -> Self {
        Self {
            test: vec!["NONE".to_string()],
            ..Default::default()
        }
    }
}

/// A device mapped from the host into the container.
///
/// Re-exported from [`super::docker_opts`], where `parse_device` builds it, so
/// that a caller has one import for the whole type rather than two.
pub use super::docker_opts::DeviceMapping;

/// `LogConfig`: the driver name and its options.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct LogConfig {
    /// The driver name, e.g. `json-file` or `none`.
    #[serde(rename = "Type")]
    pub kind: String,
    /// The driver's options.
    #[serde(rename = "Config")]
    pub config: BTreeMap<String, String>,
}

/// `network.EndpointIPAMConfig`: the addresses assigned to one endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EndpointIpamConfig {
    /// The requested IPv4 address, absent for "any".
    pub ipv4_address: Option<std::net::IpAddr>,
    /// The requested IPv6 address, absent for "any".
    pub ipv6_address: Option<std::net::IpAddr>,
    /// Requested IPv4/IPv6 link-local addresses.
    pub link_local_ips: Vec<std::net::IpAddr>,
}

impl Serialize for EndpointIpamConfig {
    /// Go keeps these as `string` and not as an address type, so they are
    /// written in their textual form — and the two scalars are `omitempty`
    /// while the list is not.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("EndpointIPAMConfig", 3)?;
        if let Some(address) = &self.ipv4_address {
            state.serialize_field("IPv4Address", &address.to_string())?;
        }
        if let Some(address) = &self.ipv6_address {
            state.serialize_field("IPv6Address", &address.to_string())?;
        }
        let links: Vec<String> = self
            .link_local_ips
            .iter()
            .map(ToString::to_string)
            .collect();
        state.serialize_field("LinkLocalIPs", &links)?;
        state.end()
    }
}

/// `network.EndpointSettings`: one network's configuration for a container.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EndpointSettings {
    /// Network-scoped aliases.
    pub aliases: Vec<String>,
    /// Driver options. `None` when none were given, which is different from an
    /// empty map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_opts: Option<BTreeMap<String, String>>,
    /// Legacy links to other containers.
    pub links: Vec<String>,
    /// The gateway priority.
    pub gw_priority: i64,
    /// The requested addresses, absent when none were requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipam_config: Option<EndpointIpamConfig>,
    /// The MAC address, as colon-separated uppercase-free hex.
    pub mac_address: String,
}

/// `serialize_port_set`: `nat.PortSet` on the wire.
///
/// `PortSet` is `map[Port]struct{}` and carries no `UnmarshalJSON`, so the
/// daemon will only accept the **object** form — `{"80/tcp": {}}` — and a list
/// is a parse error rather than a tolerated alternative. A `BTreeSet` in Rust
/// would serialize as a list, so the shape is built by hand here.
fn serialize_port_set<S>(
    ports: &BTreeSet<String>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut map = serializer.serialize_map(Some(ports.len()))?;
    for port in ports {
        map.serialize_entry(port, &serde_json::Value::Object(Default::default()))?;
    }
    map.end()
}

/// `container.Config`: what the container itself is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Config {
    /// `container.Hostname`.
    #[serde(rename = "Hostname")]
    pub hostname: String,
    /// `container.Domainname`.
    #[serde(rename = "Domainname")]
    pub domainname: String,
    /// The ports the container declares open.
    ///
    /// `nat.PortSet` is a `map[Port]struct{}` and has no `UnmarshalJSON`, so
    /// the daemon accepts *only* the object form — `{"80/tcp": {}}`. A list is
    /// a parse error there, so the shape is built by hand rather than left to
    /// what a `BTreeSet` would produce.
    #[serde(rename = "ExposedPorts", skip_serializing_if = "BTreeSet::is_empty")]
    #[serde(serialize_with = "serialize_port_set")]
    pub exposed_ports: BTreeSet<String>,
    /// The user the container's process runs as.
    #[serde(rename = "User")]
    pub user: String,
    /// Whether a pseudo-TTY is allocated.
    #[serde(rename = "Tty")]
    pub tty: bool,
    /// Whether stdin stays open.
    #[serde(rename = "OpenStdin")]
    pub open_stdin: bool,
    /// Whether stdin is attached.
    #[serde(rename = "AttachStdin")]
    pub attach_stdin: bool,
    /// Whether stdout is attached.
    #[serde(rename = "AttachStdout")]
    pub attach_stdout: bool,
    /// Whether stderr is attached.
    #[serde(rename = "AttachStderr")]
    pub attach_stderr: bool,
    /// Whether stdin closes when the client disconnects.
    #[serde(rename = "StdinOnce")]
    pub stdin_once: bool,
    /// `KEY=value` pairs, in the order given.
    #[serde(rename = "Env")]
    pub env: Vec<String>,
    /// The command, as a split argv.
    #[serde(rename = "Cmd")]
    pub cmd: Vec<String>,
    /// The image reference.
    #[serde(rename = "Image")]
    pub image: String,
    /// The volumes declared in the image's config.
    ///
    /// Also a `map[string]struct{}` upstream, so it is written as an object
    /// whose values are empty.
    #[serde(rename = "Volumes")]
    pub volumes: BTreeMap<String, serde_json::Value>,
    /// The entrypoint override. `None` and `Some(vec![""])` mean different
    /// things — see [`Config::entrypoint_is_reset`].
    #[serde(rename = "Entrypoint")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<Vec<String>>,
    /// The working directory inside the container.
    #[serde(rename = "WorkingDir")]
    pub working_dir: String,
    /// Labels, as a map.
    #[serde(rename = "Labels")]
    pub labels: BTreeMap<String, String>,
    /// The signal that stops the container.
    #[serde(rename = "StopSignal")]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub stop_signal: String,
    /// Seconds to wait for a clean stop. `None` unless `--stop-timeout` was
    /// given, which is what the daemon's own default applies otherwise.
    #[serde(rename = "StopTimeout")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_timeout: Option<i64>,
    /// The health check, or `None` when the image defines none.
    #[serde(rename = "Healthcheck")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<HealthConfig>,
}

impl Config {
    /// Whether `--entrypoint=` was given empty.
    ///
    /// That resets the image's entrypoint, which is different from omitting the
    /// flag entirely. Keeping `None` and `Some(vec![""])` distinguishable is the
    /// entire reason `entrypoint` is an `Option<Vec<_>>` and not a `Vec`.
    pub fn entrypoint_is_reset(&self) -> bool {
        matches!(self.entrypoint.as_deref(), Some([entry]) if entry.is_empty())
    }
}

/// One published port: the host address and port to publish on.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct PortBinding {
    /// The host address, empty for every interface.
    #[serde(rename = "HostIp")]
    pub host_ip: String,
    /// The host port, empty to let the daemon choose.
    #[serde(rename = "HostPort")]
    pub host_port: String,
}

/// `container.Resources`: the limits and reservations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Resources {
    /// The parent cgroup.
    #[serde(rename = "CgroupParent")]
    pub cgroup_parent: String,
    /// The memory limit in bytes.
    #[serde(rename = "Memory")]
    pub memory: i64,
    /// The soft memory limit in bytes.
    #[serde(rename = "MemoryReservation")]
    pub memory_reservation: i64,
    /// The memory-plus-swap limit in bytes.
    #[serde(rename = "MemorySwap")]
    pub memory_swap: i64,
    /// `&copts.swappiness` — a *pointer* upstream, because `-1` means unset.
    #[serde(rename = "MemorySwappiness")]
    pub memory_swappiness: i64,
    /// Whether the OOM killer is disabled.
    #[serde(rename = "OomKillDisable")]
    pub oom_kill_disable: bool,
    /// CPU quota in nano-CPUs.
    #[serde(rename = "NanoCpus")]
    pub nano_cpus: i64,
    /// Windows CPU count.
    #[serde(rename = "CpuCount")]
    pub cpu_count: i64,
    /// Windows CPU percent.
    #[serde(rename = "CpuPercent")]
    pub cpu_percent: i64,
    /// Relative CPU weight.
    #[serde(rename = "CpuShares")]
    pub cpu_shares: i64,
    /// CFS period.
    #[serde(rename = "CpuPeriod")]
    pub cpu_period: i64,
    /// Allowed CPUs.
    #[serde(rename = "CpusetCpus")]
    pub cpuset_cpus: String,
    /// Allowed memory nodes.
    #[serde(rename = "CpusetMems")]
    pub cpuset_mems: String,
    /// CFS quota.
    #[serde(rename = "CpuQuota")]
    pub cpu_quota: i64,
    /// Real-time period.
    #[serde(rename = "CpuRealtimePeriod")]
    pub cpu_realtime_period: i64,
    /// Real-time runtime.
    #[serde(rename = "CpuRealtimeRuntime")]
    pub cpu_realtime_runtime: i64,
    /// PIDs limit.
    #[serde(rename = "PidsLimit")]
    pub pids_limit: i64,
    /// Block IO weight.
    #[serde(rename = "BlkioWeight")]
    pub blkio_weight: u32,
    /// Per-device block IO weights.
    #[serde(rename = "BlkioWeightDevice")]
    pub blkio_weight_device: Vec<super::docker_opts_types::WeightDevice>,
    /// Per-device read bandwidth limits.
    #[serde(rename = "BlkioDeviceReadBps")]
    pub blkio_device_read_bps: Vec<super::docker_opts_types::ThrottleDevice>,
    /// Per-device write bandwidth limits.
    #[serde(rename = "BlkioDeviceWriteBps")]
    pub blkio_device_write_bps: Vec<super::docker_opts_types::ThrottleDevice>,
    /// Per-device read IOPS limits.
    #[serde(rename = "BlkioDeviceReadIOps")]
    pub blkio_device_read_iops: Vec<super::docker_opts_types::ThrottleDevice>,
    /// Per-device write IOPS limits.
    #[serde(rename = "BlkioDeviceWriteIOps")]
    pub blkio_device_write_iops: Vec<super::docker_opts_types::ThrottleDevice>,
    /// Windows IO bandwidth limit.
    #[serde(rename = "IOMaximumBandwidth")]
    pub io_maximum_bandwidth: i64,
    /// Windows IOps limit.
    #[serde(rename = "IOMaximumIOps")]
    pub io_maximum_iops: i64,
    /// The `--ulimit` entries.
    #[serde(rename = "Ulimits")]
    pub ulimits: Vec<Ulimit>,
    /// The device cgroup rules.
    #[serde(rename = "DeviceCgroupRules")]
    pub device_cgroup_rules: Vec<String>,
    /// Host devices mapped in.
    #[serde(rename = "Devices")]
    pub devices: Vec<DeviceMapping>,
    /// Device requests, from `--gpus` and from CDI names.
    #[serde(rename = "DeviceRequests")]
    pub device_requests: Vec<DeviceRequest>,
}

/// `container.HostConfig`: how the daemon should run the container.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct HostConfig {
    /// `host:container` bind specifications, in the order given.
    #[serde(rename = "Binds")]
    pub binds: Vec<String>,
    /// Where to write the container id.
    #[serde(rename = "ContainerIDFile")]
    pub container_id_file: String,
    /// The host's OOM score adjustment to write.
    #[serde(rename = "OomScoreAdj")]
    pub oom_score_adj: i64,
    /// Whether the container removes itself on exit.
    #[serde(rename = "AutoRemove")]
    pub auto_remove: bool,
    /// Whether it runs with extended privileges.
    #[serde(rename = "Privileged")]
    pub privileged: bool,
    /// Published ports, per container port.
    #[serde(rename = "PortBindings")]
    pub port_bindings: BTreeMap<String, Vec<PortBinding>>,
    /// Legacy `--link` entries.
    #[serde(rename = "Links")]
    pub links: Vec<String>,
    /// Whether every exposed port is published.
    #[serde(rename = "PublishAllPorts")]
    pub publish_all_ports: bool,
    /// DNS servers. Never empty-and-nil: a pre-created container can still have
    /// nil here, and the daemon rejects that on update.
    #[serde(rename = "Dns")]
    pub dns: Vec<String>,
    /// DNS search domains.
    #[serde(rename = "DnsSearch")]
    pub dns_search: Vec<String>,
    /// DNS resolver options.
    #[serde(rename = "DnsOptions")]
    pub dns_options: Vec<String>,
    /// `--add-host` entries.
    #[serde(rename = "ExtraHosts")]
    pub extra_hosts: Vec<String>,
    /// `--volumes-from` entries.
    #[serde(rename = "VolumesFrom")]
    pub volumes_from: Vec<String>,
    /// The IPC namespace mode.
    #[serde(rename = "IpcMode")]
    pub ipc_mode: String,
    /// The network mode.
    #[serde(rename = "NetworkMode")]
    pub network_mode: String,
    /// The PID namespace mode.
    #[serde(rename = "PidMode")]
    pub pid_mode: PidMode,
    /// The UTS namespace mode.
    #[serde(rename = "UTSMode")]
    pub uts_mode: UtsMode,
    /// The user namespace mode.
    #[serde(rename = "UsernsMode")]
    pub userns_mode: UsernsMode,
    /// The cgroup namespace mode.
    #[serde(rename = "CgroupnsMode")]
    pub cgroupns_mode: CgroupnsMode,
    /// Capabilities to add.
    #[serde(rename = "CapAdd")]
    pub cap_add: Vec<String>,
    /// Capabilities to drop.
    #[serde(rename = "CapDrop")]
    pub cap_drop: Vec<String>,
    /// Additional groups to join.
    #[serde(rename = "GroupAdd")]
    pub group_add: Vec<String>,
    /// The restart policy.
    #[serde(rename = "RestartPolicy")]
    pub restart_policy: RestartPolicy,
    /// The security options, minus any handled client-side.
    #[serde(rename = "SecurityOpt")]
    pub security_opt: Vec<String>,
    /// The storage driver options.
    #[serde(rename = "StorageOpt")]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub storage_opt: BTreeMap<String, String>,
    /// Whether the root filesystem is mounted read-only.
    #[serde(rename = "ReadonlyRootfs")]
    pub readonly_rootfs: bool,
    /// The logging driver and its options.
    #[serde(rename = "LogConfig")]
    pub log_config: LogConfig,
    /// The volume driver.
    #[serde(rename = "VolumeDriver")]
    pub volume_driver: String,
    /// The isolation technology.
    #[serde(rename = "Isolation")]
    pub isolation: Isolation,
    /// The size of `/dev/shm`.
    #[serde(rename = "ShmSize")]
    pub shm_size: i64,
    /// The resource limits.
    #[serde(flatten)]
    pub resources: Resources,
    /// `--tmpfs` mounts.
    #[serde(rename = "Tmpfs")]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub tmpfs: BTreeMap<String, String>,
    /// `--sysctl` entries.
    #[serde(rename = "Sysctls")]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub sysctls: BTreeMap<String, String>,
    /// The OCI runtime.
    #[serde(rename = "Runtime")]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub runtime: String,
    /// `--mount` entries.
    #[serde(rename = "Mounts")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<super::docker_opts_mounts::mount::Mount>,
    /// Paths masked from the container. `None` means "the daemon's default",
    /// which is different from an empty list.
    #[serde(rename = "MaskedPaths")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_paths: Option<Vec<String>>,
    /// Paths read-only in the container, same distinction.
    #[serde(rename = "ReadonlyPaths")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly_paths: Option<Vec<String>>,
    /// `--annotation` entries.
    #[serde(rename = "Annotations")]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
    /// Whether an init process runs. `None` unless `--init` was given.
    #[serde(rename = "Init")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init: Option<bool>,
}

/// What `parse()` produces: the structures the daemon is asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContainerConfig {
    /// What the container is.
    pub config: Config,
    /// How the daemon should run it.
    pub host_config: HostConfig,
    /// The network endpoints, keyed by network name.
    ///
    /// The value is an `Option` because a network may legitimately end up with
    /// *no* endpoint configuration at all, and upstream distinguishes that from
    /// an empty one — see `parse_network_opts`.
    pub endpoints: BTreeMap<String, Option<EndpointSettings>>,
}

impl fmt::Display for PidMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // docker_cli_test.go: TestParseModes
    #[test]
    fn a_namespace_mode_is_validated_per_namespace() {
        // pid accepts a container, but only with an id after the colon.
        assert!(PidMode("host".to_string()).valid());
        assert!(PidMode("container:abc".to_string()).valid());
        assert!(!PidMode("container:".to_string()).valid());
        assert!(!PidMode("private".to_string()).valid());
        assert!(PidMode(String::new()).valid());

        // uts has no container form at all — the easy mistake.
        assert!(UtsMode("host".to_string()).valid());
        assert!(!UtsMode("container:abc".to_string()).valid());
        assert!(!UtsMode("container:".to_string()).valid());
        assert!(UtsMode(String::new()).valid());

        // userns likewise.
        assert!(UsernsMode("host".to_string()).valid());
        assert!(!UsernsMode("private".to_string()).valid());

        // cgroupns has its own vocabulary.
        assert!(CgroupnsMode("private".to_string()).valid());
        assert!(CgroupnsMode("host".to_string()).valid());
        assert!(!CgroupnsMode("shareable".to_string()).valid());

        // ipc accepts the widest set, including a bare container prefix.
        for mode in [
            "",
            "none",
            "private",
            "host",
            "shareable",
            "container:abc",
            "container:",
        ] {
            assert!(IpcMode(mode.to_string()).valid(), "{mode} should be valid");
        }
        assert!(!IpcMode("bogus".to_string()).valid());
    }

    /// `IsContainer` and `Valid` disagree for a bare prefix, because only the
    /// former is asked "is this a container mode" and the latter also wants an
    /// id. Two upstream functions, two answers.
    #[test]
    fn an_ipc_container_prefix_is_recognised_without_an_id() {
        let bare = IpcMode("container:".to_string());
        assert!(bare.is_container(), "the prefix is there");
        assert!(bare.container().is_some());
    }

    #[test]
    fn a_pid_mode_names_the_container_it_shares_with() {
        assert_eq!(
            PidMode("container:mybox".to_string()).container(),
            Some("mybox")
        );
        assert_eq!(PidMode("host".to_string()).container(), None);
    }

    /// The empty name counts as disabled, which is what lets `--rm` combine
    /// with a default restart policy.
    #[test]
    fn an_unset_restart_policy_counts_as_none() {
        assert!(RestartPolicy::default().is_none());
        assert!(RestartPolicy {
            name: RESTART_POLICY_DISABLED.to_string(),
            maximum_retry_count: 0,
        }
        .is_none());
        assert!(!RestartPolicy {
            name: RESTART_POLICY_ALWAYS.to_string(),
            maximum_retry_count: 0,
        }
        .is_none());
    }

    #[test]
    fn isolation_compares_case_insensitively() {
        assert!(Isolation("default".to_string()).is_default());
        assert!(Isolation("Default".to_string()).is_default());
        assert!(!Isolation("hyperv".to_string()).is_default());
    }

    #[test]
    fn a_disabled_healthcheck_is_the_none_probe() {
        assert_eq!(HealthConfig::none().test, vec!["NONE".to_string()]);
    }
}
