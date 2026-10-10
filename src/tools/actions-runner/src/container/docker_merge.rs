//! `mergo.Merge(dst, src, mergo.WithOverride)`, as act uses it.
//!
//! `mergeContainerConfigs` folds a job's `options:` into the config the runner
//! built for the container. It does that with `dario.cat/mergo`, and mergo's
//! `WithOverride` is **not** "source wins". It is:
//!
//! > overwrite `dst` with `src`, but only where `src` is *non-zero*.
//!
//! A field is zero when it is the empty string, `false`, `0`, an empty or nil
//! slice, a nil map, or a nil pointer. So `options: --tty=false` does **not**
//! turn a TTY off — `false` is mergo's zero value and the field is skipped. The
//! same is true of every unset string, every `0` limit, and every empty list.
//!
//! Three further measured behaviours, all of which the port depends on:
//!
//! | kind | behaviour |
//! |---|---|
//! | slice | **replaced** wholesale, not appended |
//! | map | merged key by key, `src` winning per key |
//! | struct | recursed field by field |
//!
//! The slice rule is why upstream appends `Binds` and `Mounts` by hand before
//! merging and restores them afterwards: without that dance a `--mount` in
//! `options:` would erase the runner's own workspace bind. See
//! [`merge_host_config`].
//!
//! Everything here is pure, so all of it is tested without a daemon — which is
//! the only part of `create` that can be.

use std::collections::BTreeMap;

use super::docker_api::{Config, HostConfig};

/// Overwrite `dst` where `src` is non-empty.
fn merge_str(dst: &mut String, src: &str) {
    if !src.is_empty() {
        dst.clear();
        dst.push_str(src);
    }
}

/// Overwrite `dst` where `src` is `true`. **`false` never overwrites.**
///
/// This is the rule that makes `--tty=false`, `--privileged=false` and
/// `--read-only=false` into no-ops when merged onto a config that has them on.
fn merge_bool(dst: &mut bool, src: bool) {
    if src {
        *dst = true;
    }
}

/// Replace `dst` where `src` is non-empty. Never appends — see the module note.
fn merge_vec<T: Clone>(dst: &mut Vec<T>, src: &[T]) {
    if !src.is_empty() {
        *dst = src.to_vec();
    }
}

/// Merge `src` into `dst` key by key; `src` wins where both have the key.
fn merge_map<V: Clone>(dst: &mut BTreeMap<String, V>, src: &BTreeMap<String, V>) {
    for (key, value) in src {
        dst.insert(key.clone(), value.clone());
    }
}

/// Overwrite where `src` is non-zero. `0` never overwrites, which is why
/// `--memory 0` cannot lift a limit the runner had already set.
fn merge_num(dst: &mut i64, src: i64) {
    if src != 0 {
        *dst = src;
    }
}

/// Overwrite where `src` is `Some`. `None` is mergo's nil pointer: skipped.
///
/// The `Option` fields upstream keeps as pointers — `init`, `masked_paths`,
/// `readonly_paths`, `stop_timeout` — map onto this exactly, with no
/// "is it empty" judgement to make.
fn merge_option<T: Clone>(dst: &mut Option<T>, src: &Option<T>) {
    if src.is_some() {
        *dst = src.clone();
    }
}

/// `mergo.Merge(config, containerConfig.Config, WithOverride)`.
///
/// The job's `options:` overlay onto the config the runner built.
pub fn merge_config(dst: &mut Config, src: &Config) {
    merge_str(&mut dst.hostname, &src.hostname);
    merge_str(&mut dst.domainname, &src.domainname);
    if !src.exposed_ports.is_empty() {
        dst.exposed_ports = src.exposed_ports.clone();
    }
    merge_str(&mut dst.user, &src.user);
    merge_bool(&mut dst.tty, src.tty);
    merge_bool(&mut dst.open_stdin, src.open_stdin);
    merge_bool(&mut dst.attach_stdin, src.attach_stdin);
    merge_bool(&mut dst.attach_stdout, src.attach_stdout);
    merge_bool(&mut dst.attach_stderr, src.attach_stderr);
    merge_bool(&mut dst.stdin_once, src.stdin_once);
    merge_vec(&mut dst.env, &src.env);
    merge_vec(&mut dst.cmd, &src.cmd);
    merge_str(&mut dst.image, &src.image);
    merge_map(&mut dst.volumes, &src.volumes);
    merge_option(&mut dst.entrypoint, &src.entrypoint);
    merge_str(&mut dst.working_dir, &src.working_dir);
    merge_map(&mut dst.labels, &src.labels);
    merge_str(&mut dst.stop_signal, &src.stop_signal);
    merge_option(&mut dst.stop_timeout, &src.stop_timeout);
    merge_option(&mut dst.healthcheck, &src.healthcheck);
}

/// The same for `HostConfig`, with `Binds` and `Mounts` **appended**.
///
/// Upstream snapshots the two, merges, and puts the snapshots back — so the
/// result is `dst ++ src` rather than the `src`-only that the plain slice rule
/// would give. The workspace bind the runner adds must survive a `--mount` in
/// `options:`, and this is the line of Go that guarantees it.
pub fn merge_host_config(dst: &mut HostConfig, src: &HostConfig) {
    dst.binds.extend(src.binds.iter().cloned());
    dst.mounts.extend(src.mounts.iter().cloned());

    merge_str(&mut dst.container_id_file, &src.container_id_file);
    merge_num(&mut dst.oom_score_adj, src.oom_score_adj);
    merge_bool(&mut dst.auto_remove, src.auto_remove);
    merge_bool(&mut dst.privileged, src.privileged);
    merge_map(&mut dst.port_bindings, &src.port_bindings);
    merge_vec(&mut dst.links, &src.links);
    merge_bool(&mut dst.publish_all_ports, src.publish_all_ports);
    merge_vec(&mut dst.dns, &src.dns);
    merge_vec(&mut dst.dns_search, &src.dns_search);
    merge_vec(&mut dst.dns_options, &src.dns_options);
    merge_vec(&mut dst.extra_hosts, &src.extra_hosts);
    merge_vec(&mut dst.volumes_from, &src.volumes_from);
    merge_str(&mut dst.ipc_mode, &src.ipc_mode);
    merge_str(&mut dst.network_mode, &src.network_mode);
    merge_str(&mut dst.pid_mode.0, &src.pid_mode.0);
    merge_str(&mut dst.uts_mode.0, &src.uts_mode.0);
    merge_str(&mut dst.userns_mode.0, &src.userns_mode.0);
    merge_str(&mut dst.cgroupns_mode.0, &src.cgroupns_mode.0);
    // `CapAdd`/`CapDrop` are plain slices upstream, so they *replace*. The
    // capabilities the runner passes in are applied after this merge, which is
    // why a workflow's `--cap-add` and the runner's do not collide.
    merge_vec(&mut dst.cap_add, &src.cap_add);
    merge_vec(&mut dst.cap_drop, &src.cap_drop);
    merge_vec(&mut dst.group_add, &src.group_add);
    merge_str(&mut dst.restart_policy.name, &src.restart_policy.name);
    merge_num(
        &mut dst.restart_policy.maximum_retry_count,
        src.restart_policy.maximum_retry_count,
    );
    merge_vec(&mut dst.security_opt, &src.security_opt);
    merge_map(&mut dst.storage_opt, &src.storage_opt);
    merge_bool(&mut dst.readonly_rootfs, src.readonly_rootfs);
    merge_str(&mut dst.log_config.kind, &src.log_config.kind);
    merge_map(&mut dst.log_config.config, &src.log_config.config);
    merge_str(&mut dst.volume_driver, &src.volume_driver);
    merge_str(&mut dst.isolation.0, &src.isolation.0);
    merge_num(&mut dst.shm_size, src.shm_size);
    merge_str(&mut dst.runtime, &src.runtime);
    merge_map(&mut dst.tmpfs, &src.tmpfs);
    merge_map(&mut dst.sysctls, &src.sysctls);
    merge_option(&mut dst.masked_paths, &src.masked_paths);
    merge_option(&mut dst.readonly_paths, &src.readonly_paths);
    merge_map(&mut dst.annotations, &src.annotations);
    merge_option(&mut dst.init, &src.init);

    let resources = &src.resources;
    merge_str(&mut dst.resources.cgroup_parent, &resources.cgroup_parent);
    merge_num(&mut dst.resources.memory, resources.memory);
    merge_num(
        &mut dst.resources.memory_reservation,
        resources.memory_reservation,
    );
    merge_num(&mut dst.resources.memory_swap, resources.memory_swap);
    merge_num(&mut dst.resources.memory_swappiness, resources.memory_swappiness);
    merge_bool(&mut dst.resources.oom_kill_disable, resources.oom_kill_disable);
    merge_num(&mut dst.resources.nano_cpus, resources.nano_cpus);
    merge_num(&mut dst.resources.cpu_count, resources.cpu_count);
    merge_num(&mut dst.resources.cpu_percent, resources.cpu_percent);
    merge_num(&mut dst.resources.cpu_shares, resources.cpu_shares);
    merge_num(&mut dst.resources.cpu_period, resources.cpu_period);
    merge_str(&mut dst.resources.cpuset_cpus, &resources.cpuset_cpus);
    merge_str(&mut dst.resources.cpuset_mems, &resources.cpuset_mems);
    merge_num(&mut dst.resources.cpu_quota, resources.cpu_quota);
    merge_num(
        &mut dst.resources.cpu_realtime_period,
        resources.cpu_realtime_period,
    );
    merge_num(
        &mut dst.resources.cpu_realtime_runtime,
        resources.cpu_realtime_runtime,
    );
    merge_num(&mut dst.resources.pids_limit, resources.pids_limit);
    if resources.blkio_weight != 0 {
        dst.resources.blkio_weight = resources.blkio_weight;
    }
    merge_vec(&mut dst.resources.blkio_weight_device, &resources.blkio_weight_device);
    merge_vec(
        &mut dst.resources.blkio_device_read_bps,
        &resources.blkio_device_read_bps,
    );
    merge_vec(
        &mut dst.resources.blkio_device_write_bps,
        &resources.blkio_device_write_bps,
    );
    merge_vec(
        &mut dst.resources.blkio_device_read_iops,
        &resources.blkio_device_read_iops,
    );
    merge_vec(
        &mut dst.resources.blkio_device_write_iops,
        &resources.blkio_device_write_iops,
    );
    merge_num(
        &mut dst.resources.io_maximum_bandwidth,
        resources.io_maximum_bandwidth,
    );
    merge_num(&mut dst.resources.io_maximum_iops, resources.io_maximum_iops);
    merge_vec(&mut dst.resources.ulimits, &resources.ulimits);
    merge_vec(
        &mut dst.resources.device_cgroup_rules,
        &resources.device_cgroup_rules,
    );
    merge_vec(&mut dst.resources.devices, &resources.devices);
    merge_vec(&mut dst.resources.device_requests, &resources.device_requests);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::docker_api::RestartPolicy;

    /// The central fact, and the one a "source wins" implementation gets
    /// wrong: a zero value in `src` leaves `dst` alone.
    ///
    /// Measured against `dario.cat/mergo v1.0.2` with `WithOverride`.
    #[test]
    fn a_zero_value_in_the_source_never_overwrites() {
        let mut dst = Config {
            image: "base".to_string(),
            env: vec!["A=1".to_string()],
            cmd: vec!["base-cmd".to_string()],
            working_dir: "/w".to_string(),
            tty: true,
            ..Default::default()
        };
        merge_config(&mut dst, &Config::default());
        assert_eq!(dst.image, "base");
        assert_eq!(dst.env, vec!["A=1".to_string()]);
        assert_eq!(dst.cmd, vec!["base-cmd".to_string()]);
        assert_eq!(dst.working_dir, "/w");
        assert!(dst.tty, "false did not turn a TTY off");
    }

    /// A non-zero source value *does* overwrite — measured in the same probe.
    #[test]
    fn a_non_zero_value_in_the_source_overwrites() {
        let mut dst = Config {
            image: "base".to_string(),
            env: vec!["A=1".to_string()],
            cmd: vec!["base-cmd".to_string()],
            working_dir: "/w".to_string(),
            tty: true,
            ..Default::default()
        };
        let src = Config {
            image: "over".to_string(),
            cmd: vec!["src-cmd".to_string()],
            working_dir: "/ws".to_string(),
            ..Default::default()
        };
        merge_config(&mut dst, &src);
        assert_eq!(dst.image, "over");
        assert_eq!(dst.cmd, vec!["src-cmd".to_string()], "a slice is replaced");
        assert_eq!(dst.working_dir, "/ws");
        assert_eq!(dst.env, vec!["A=1".to_string()], "an empty source slice skips");
        assert!(dst.tty, "false in the source skips");
    }

    /// Maps merge per key, and both keys survive. Measured:
    /// `{a:1,b:2}` + `{b:override,c:3}` → `{a:1, b:override, c:3}`.
    #[test]
    fn maps_merge_key_by_key_rather_than_replacing() {
        let mut dst = BTreeMap::from([
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string()),
        ]);
        let src = BTreeMap::from([
            ("b".to_string(), "override".to_string()),
            ("c".to_string(), "3".to_string()),
        ]);
        merge_map(&mut dst, &src);
        assert_eq!(dst.get("a").map(String::as_str), Some("1"), "kept");
        assert_eq!(dst.get("b").map(String::as_str), Some("override"), "won");
        assert_eq!(dst.get("c").map(String::as_str), Some("3"), "added");
    }

    /// A nested struct is recursed, not replaced: one zero field does not
    /// clear its siblings. Measured: `Memory` stayed 100 while `NanoCPUs` went
    /// 200 → 999.
    #[test]
    fn a_nested_struct_is_merged_field_by_field() {
        let mut dst = HostConfig {
            resources: super::super::docker_api::Resources {
                memory: 100,
                nano_cpus: 200,
                ..Default::default()
            },
            ..Default::default()
        };
        let src = HostConfig {
            resources: super::super::docker_api::Resources {
                nano_cpus: 999,
                ..Default::default()
            },
            ..Default::default()
        };
        merge_host_config(&mut dst, &src);
        assert_eq!(dst.resources.memory, 100, "an untouched zero does not clear");
        assert_eq!(dst.resources.nano_cpus, 999);
    }

    /// The `Binds`/`Mounts` append-and-restore. Without it, a `--mount` in
    /// `options:` would wipe the runner's own workspace bind — which is the
    /// single most load-bearing line in this module.
    #[test]
    fn binds_and_mounts_are_appended_not_replaced() {
        let mut dst = HostConfig {
            binds: vec!["/workspace:/github/workspace".to_string()],
            ..Default::default()
        };
        let src = HostConfig {
            binds: vec!["/cache:/cache".to_string()],
            ..Default::default()
        };
        merge_host_config(&mut dst, &src);
        assert_eq!(
            dst.binds,
            vec![
                "/workspace:/github/workspace".to_string(),
                "/cache:/cache".to_string()
            ],
            "the runner's bind comes first, the option's is appended"
        );
    }

    /// Every other slice *replaces*, which is the opposite rule and the easy
    /// one to apply by accident.
    #[test]
    fn other_slices_replace_rather_than_append() {
        let mut dst = HostConfig {
            dns: vec!["1.1.1.1".to_string()],
            cap_add: vec!["SYS_ADMIN".to_string()],
            ..Default::default()
        };
        let src = HostConfig {
            dns: vec!["8.8.8.8".to_string()],
            cap_add: vec!["NET_ADMIN".to_string()],
            ..Default::default()
        };
        merge_host_config(&mut dst, &src);
        assert_eq!(dst.dns, vec!["8.8.8.8".to_string()]);
        assert_eq!(dst.cap_add, vec!["NET_ADMIN".to_string()]);
    }

    /// A `RestartPolicy` is a nested struct, so `--restart=on-failure:5` sets
    /// the name *and* the count, and `--restart=always` does not reset a count
    /// that is already there.
    #[test]
    fn the_restart_policy_is_merged_not_replaced() {
        let mut dst = HostConfig {
            restart_policy: RestartPolicy {
                name: "on-failure".to_string(),
                maximum_retry_count: 5,
            },
            ..Default::default()
        };
        merge_host_config(
            &mut dst,
            &HostConfig {
                restart_policy: RestartPolicy {
                    name: "always".to_string(),
                    maximum_retry_count: 0,
                },
                ..Default::default()
            },
        );
        assert_eq!(dst.restart_policy.name, "always");
        assert_eq!(
            dst.restart_policy.maximum_retry_count, 5,
            "a zero count does not clear the one already set"
        );
    }

    /// The `Option` fields are mergo's nil pointers, so `None` skips and
    /// `Some` writes. This is what lets `options:` switch the daemon's own
    /// default for `masked_paths` off without inventing an "empty vs unset"
    /// rule.
    #[test]
    fn an_option_field_writes_on_some_and_skips_on_none() {
        let mut dst = HostConfig {
            init: Some(false),
            masked_paths: Some(vec!["/proc/kcore".to_string()]),
            ..Default::default()
        };
        merge_host_config(
            &mut dst,
            &HostConfig {
                init: Some(true),
                ..Default::default()
            },
        );
        assert_eq!(dst.init, Some(true));
        assert_eq!(
            dst.masked_paths,
            Some(vec!["/proc/kcore".to_string()]),
            "None in the source leaves the destination alone"
        );
    }
}
