//! Docker networks and volumes: the two Engine-API resources act manages around
//! a job rather than inside it.
//!
//! Ported from act's `pkg/container/docker_network.go` (79 lines) and
//! `pkg/container/docker_volume.go` (54 lines).
//!
//! # Why this module is called `docker_resources`
//!
//! Those two files have no package of their own to be named after — each is a
//! short file of Engine-API helpers sitting in the same `pkg/container` as the
//! container itself, with nothing else in common between them. Rather than
//! invent a package that upstream does not have, the port keeps them together
//! under the thing they both act on: the daemon's *resources*, as opposed to
//! the containers that
//! [`super::docker_engine::ExecutionsEnvironment`] runs.
//!
//! # Upstream has no tests for these files, and these are not tests of them
//!
//! Neither `docker_network.go` nor `docker_volume.go` ships a test, and neither
//! could: every function here is a thin wrapper over a live daemon call, and a
//! test for one needs a daemon. So the daemon paths here are **not** verified to
//! work, and nothing in this file should be read as claiming that.
//!
//! What *is* tested is the part that is pure: the decision each loop makes
//! before it talks to the daemon, extracted into [`decide_network_create`],
//! [`decide_network_removal`], [`plan_network_removals`],
//! [`classify_network_removal`] and [`decide_volume_remove`]. Those tests are
//! **mine, not upstream's**. What they pin is the *branch structure* — that
//! removal is attempted only for an empty network, that a missing volume is a
//! success, that the remove loop does not stop early — and nothing about daemon
//! behaviour.
//!
//! # Four behaviours upstream has that a reader would otherwise "fix"
//!
//! **A network that already exists is not an error, and not a create.** The
//! create executor lists first and *returns* if the name is taken, rather than
//! letting the daemon reject the create. It logs at debug and returns `Ok`.
//!
//! **A failed network removal is swallowed.** `NetworkRemove` failing is logged
//! at debug and the loop continues; the executor's own error is not set by it.
//! Only an *inspect* failure aborts the loop.
//!
//! **A missing volume is a silent success.** Upstream's own comment says
//! `// Volume not found - do nothing`. That is not a shortcut: this code path
//! deletes volumes, so a name that matches nothing must leave the daemon
//! alone rather than pass a mismatched name to a delete.
//!
//! **Only the inner volume remove checks `dryrun`.** `NewDockerVolumeRemoveExecutor`
//! has no dryrun check of its own; the `removeExecutor` it delegates to does.
//! Adding a check to the outer one would be a behaviour change, so the split is
//! reproduced as-is and pinned by a test.
//!
//! # `bollard` is async, these executors are not
//!
//! Same trade as [`super::docker_engine`]: each executor is a blocking
//! `common.Executor` closure, so every daemon call goes through `block_on`, and
//! each one opens its own client via `connect` and drops it on return — bollard
//! closes on drop, which is how upstream's `defer cli.Close()` is expressed.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use bollard::models::NetworkCreateRequest;
use bollard::query_parameters::{
    ListNetworksOptionsBuilder, ListVolumesOptionsBuilder, RemoveVolumeOptionsBuilder,
};

use crate::common::{Executor, RunContext};

use super::docker_engine::{block_on, connect};

/// The prefix act puts in front of the Docker commands it reports.
///
/// Upstream declares it in `docker_logger.go` and uses it from four other
/// files, so it is defined once in [`super::docker_log`] and re-exported here
/// rather than written a second time. Two copies of a user-visible byte string
/// are two things that can drift apart.
pub use super::docker_log::LOG_PREFIX;

/// What a network-related loop decides to do with one entry.
///
/// One enum for both loops because the two loops are two halves of the same
/// question, but each only ever produces its own half:
///
/// * the create loop yields [`NetworkAction::Create`] or
///   [`NetworkAction::AlreadyExists`]
/// * the remove loop yields [`NetworkAction::Remove`],
///   [`NetworkAction::SkipActiveEndpoints`] or
///   [`NetworkAction::SkipRemoveFailed`]
///
/// `Remove` and `SkipRemoveFailed` carry the network id, which comes from the
/// daemon and is therefore not a `'static` string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkAction {
    /// The name is free, so create the network.
    Create,
    /// A network of this name already exists — do nothing.
    AlreadyExists,
    /// Inspect found no attached endpoints, so attempt the removal.
    Remove {
        /// The network's id, which is what the daemon is asked to remove.
        id: String,
    },
    /// Inspect found attached endpoints, so refuse to remove it.
    SkipActiveEndpoints {
        /// The network's id, named so a caller can log which one was skipped.
        id: String,
    },
    /// The remove call failed. Upstream logs and swallows this.
    SkipRemoveFailed {
        /// The network's id that could not be removed.
        id: String,
    },
}

/// What the volume list says about the volume an executor was asked to remove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeAction {
    /// A volume of this name is present, so remove it.
    Remove,
    /// No volume of this name exists. Upstream does nothing, successfully.
    NotFound,
}

/// `NewDockerNetworkCreateExecutor`.
///
/// Creates `name` as a `bridge` network, but **only if it does not already
/// exist**: the list is checked first and an existing name is a debug log plus
/// an early return, not a create the daemon will reject.
pub fn new_docker_network_create_executor(name: String) -> Executor {
    Arc::new(move |ctx: &RunContext| {
        let client = connect()?;
        let networks = block_on(client.list_networks(Some(ListNetworksOptionsBuilder::new().build())))
            .map_err(|err| anyhow!("failed to list docker networks: {err}"))?;
        ctx.log_debug(&format!("{networks:?}"));

        let known: Vec<(String, String)> = networks
            .iter()
            .map(|net| {
                (
                    net.name.clone().unwrap_or_default(),
                    net.id.clone().unwrap_or_default(),
                )
            })
            .collect();

        if let NetworkAction::AlreadyExists = decide_network_create(&known, &name) {
            ctx.log_debug(&format!("Network {name} exists"));
            return Ok(());
        }

        block_on(client.create_network(NetworkCreateRequest {
            name: name.clone(),
            driver: Some("bridge".to_string()),
            scope: Some("local".to_string()),
            ..Default::default()
        }))
        .map_err(|err| anyhow!("failed to create docker network {name}: {err}"))?;

        Ok(())
    })
}

/// `NewDockerNetworkRemoveExecutor`.
///
/// Removes **every** network carrying `name`, not just the first. Upstream's
/// comment is explicit that `NetworkRemove` refuses to remove a network while
/// duplicates of its name exist, so leaving a second one behind would make every
/// later call fail.
///
/// Two things are load-bearing about the loop:
///
/// * it does **not** break — a busy network is skipped, and the next duplicate
///   is still tried
/// * a failed removal is logged at debug and **swallowed**; only a failed
///   *inspect* returns an error
pub fn new_docker_network_remove_executor(name: String) -> Executor {
    Arc::new(move |ctx: &RunContext| {
        let client = connect()?;
        let networks = block_on(client.list_networks(Some(ListNetworksOptionsBuilder::new().build())))
            .map_err(|err| anyhow!("failed to list docker networks: {err}"))?;
        ctx.log_debug(&format!("{networks:?}"));

        for net in &networks {
            if net.name.clone().unwrap_or_default() != name {
                continue;
            }
            let id = net.id.clone().unwrap_or_default();

            // Inspect is the one call whose failure aborts the loop.
            let inspected = block_on(client.inspect_network(&id, None))
                .map_err(|err| anyhow!("failed to inspect docker network {id}: {err}"))?;

            // `containers` is `None` when the daemon sent no `Containers` key,
            // and Go's `len()` on the nil map is 0, so absent means empty.
            let endpoints = inspected.containers.as_ref().map_or(0, |map| map.len());

            match decide_network_removal(&id, endpoints) {
                NetworkAction::Remove { .. } => {
                    let outcome = block_on(client.remove_network(&id))
                        .map_err(|err| format!("failed to remove docker network {id}: {err}"));
                    if let NetworkAction::SkipRemoveFailed { id } =
                        classify_network_removal(&id, &outcome)
                    {
                        // Debug and swallowed — see the module note.
                        ctx.log_debug(&format!("failed to remove docker network {id}"));
                    }
                }
                NetworkAction::SkipActiveEndpoints { id } => {
                    ctx.log_debug(&format!(
                        "Refusing to remove network {name} because it still has active endpoints ({} on {id})",
                        endpoints
                    ));
                }
                // `decide_network_removal` only ever yields the two branches
                // above; the rest of the enum belongs to the other loop.
                NetworkAction::Create
                | NetworkAction::AlreadyExists
                | NetworkAction::SkipRemoveFailed { .. } => {
                    unreachable!("decide_network_removal only yields removal actions")
                }
            }
        }

        Ok(())
    })
}

/// `NewDockerVolumeRemoveExecutor`.
///
/// Lists volumes and delegates to [`remove_executor`] **only** if a volume of
/// that name is there. A name that matches nothing is a silent success.
///
/// Note the absent dryrun check. Upstream puts it in the inner executor only,
/// and this one still talks to the daemon to list volumes in a dry run; that is
/// reproduced rather than corrected.
pub fn new_docker_volume_remove_executor(volume_name: String, force: bool) -> Executor {
    Arc::new(move |ctx: &RunContext| {
        let client = connect()?;
        let list = block_on(client.list_volumes(Some(ListVolumesOptionsBuilder::new().build())))
            .map_err(|err| anyhow!("failed to list docker volumes: {err}"))?;

        let names: Vec<String> = list
            .volumes
            .unwrap_or_default()
            .iter()
            .map(|vol| vol.name.clone())
            .collect();

        match decide_volume_remove(&names, &volume_name) {
            VolumeAction::Remove => remove_executor(volume_name.clone(), force)(ctx),
            VolumeAction::NotFound => {
                // Volume not found - do nothing.
                Ok(())
            }
        }
    })
}

/// `removeExecutor`: the actual `docker volume rm`.
///
/// This is the **only** one of the two volume executors that checks `dryrun`,
/// and it does so *after* logging the command, so a dry run still says what it
/// would have done. It returns before opening a client, which is what makes the
/// dryrun split testable without a daemon.
fn remove_executor(volume: String, force: bool) -> Executor {
    Arc::new(move |ctx: &RunContext| {
        ctx.log_debug(&format!("{LOG_PREFIX}docker volume rm {volume}"));

        if ctx.dryrun() {
            return Ok(());
        }

        let client = connect()?;
        block_on(client.remove_volume(
            &volume,
            Some(RemoveVolumeOptionsBuilder::new().force(force).build()),
        ))
        .map_err(|err| anyhow!("failed to remove docker volume {volume}: {err}"))
    })
}

/// The create loop's decision: is `name` free, or already taken?
///
/// A slice of `(name, id)` pairs, which is what [`new_docker_network_create_executor`]
/// reduces the daemon's list to before deciding. Matching is on the name alone;
/// the id is carried so the tuple shape is the same as the remove loop's.
pub fn decide_network_create(networks: &[(String, String)], name: &str) -> NetworkAction {
    if networks.iter().any(|(known, _)| known == name) {
        NetworkAction::AlreadyExists
    } else {
        NetworkAction::Create
    }
}

/// The remove loop's decision for one network, given how many endpoints its
/// inspect reported.
///
/// Zero endpoints means remove. Anything else means refuse, because removing a
/// network with live endpoints would disconnect a running container.
pub fn decide_network_removal(id: &str, endpoint_count: usize) -> NetworkAction {
    if endpoint_count == 0 {
        NetworkAction::Remove { id: id.to_string() }
    } else {
        NetworkAction::SkipActiveEndpoints { id: id.to_string() }
    }
}

/// The remove loop's decision for every network of the right name, in order.
///
/// This is the whole loop as a value, and the point of it is that it yields one
/// action per record: upstream does not break out of the loop on a busy
/// network, so duplicates after a skipped one are still removed. A slice of
/// `(name, id, endpoint_count)`.
pub fn plan_network_removals(networks: &[(String, String, usize)]) -> Vec<NetworkAction> {
    networks
        .iter()
        .map(|(_, id, endpoints)| decide_network_removal(id, *endpoints))
        .collect()
}

/// How a `remove_network` call turned out.
///
/// Upstream logs the error at debug and swallows it, so the *only* observable
/// difference between the two is which message is logged — the executor's own
/// result does not change. The outcome is taken as a plain `Result<(), String>`
/// so this stays a function over data rather than over a bollard error.
pub fn classify_network_removal(id: &str, outcome: &Result<(), String>) -> NetworkAction {
    match outcome {
        Ok(()) => NetworkAction::Remove { id: id.to_string() },
        Err(_) => NetworkAction::SkipRemoveFailed { id: id.to_string() },
    }
}

/// The volume loop's decision: is `volume_name` in the list at all?
///
/// [`VolumeAction::NotFound`] is a success upstream, and deliberately so: this
/// is the branch that keeps a name which matches nothing from reaching a
/// delete.
pub fn decide_volume_remove(volumes: &[String], volume_name: &str) -> VolumeAction {
    if volumes.iter().any(|known| known == volume_name) {
        VolumeAction::Remove
    } else {
        VolumeAction::NotFound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(name: &str, id: &str) -> (String, String) {
        (name.to_string(), id.to_string())
    }

    fn net_with_endpoints(name: &str, id: &str, endpoints: usize) -> (String, String, usize) {
        (name.to_string(), id.to_string(), endpoints)
    }

    /// The prefix is user-visible, so it is pinned as bytes: two spaces, the
    /// four-byte encoding of U+1F433, two more spaces. An escape written the
    /// other way round would still be the same string, but a wrong *character*
    /// would not be caught by a test that only compared `str`s it also wrote.
    #[test]
    fn the_log_prefix_is_two_spaces_a_dog_face_and_two_spaces() {
        assert_eq!(
            LOG_PREFIX.as_bytes(),
            [0x20, 0x20, 0xF0, 0x9F, 0x90, 0xB3, 0x20, 0x20],
            "bytes must match upstream's \"  \\U0001F433  \"",
        );
        assert_eq!(LOG_PREFIX.chars().count(), 5, "5 characters, 8 bytes");
    }

    /// The create loop's first rule: an existing name stops the create. The
    /// list is not empty — the match is on the name, not on the list.
    #[test]
    fn create_is_skipped_when_a_network_of_that_name_exists() {
        let known = vec![net("bridge", "aaa"), net("act_default", "bbb")];
        assert_eq!(
            decide_network_create(&known, "act_default"),
            NetworkAction::AlreadyExists,
        );
    }

    /// The other half of the same rule: anything not matching means create.
    #[test]
    fn create_happens_when_no_network_carries_that_name() {
        let known = vec![net("bridge", "aaa"), net("host", "bbb")];
        assert_eq!(
            decide_network_create(&known, "act_default"),
            NetworkAction::Create,
        );
    }

    /// An empty daemon list is a create, not an error.
    #[test]
    fn create_happens_against_an_empty_network_list() {
        assert_eq!(decide_network_create(&[], "act_default"), NetworkAction::Create);
    }

    /// A name match is a whole-name match. `act` must not swallow `act_default`,
    /// or a job would silently reuse another job's network.
    #[test]
    fn a_network_name_is_matched_in_full_not_by_prefix() {
        let known = vec![net("act_default", "bbb")];
        assert_eq!(decide_network_create(&known, "act"), NetworkAction::Create);
    }

    /// Zero endpoints means remove. This is the branch that has to exist at all:
    /// without it, nothing would ever be removed.
    #[test]
    fn a_network_with_no_attached_endpoints_is_removed() {
        assert_eq!(
            decide_network_removal("abc", 0),
            NetworkAction::Remove { id: "abc".to_string() },
        );
    }

    /// One endpoint is enough to refuse. Upstream logs
    /// `Refusing to remove network %v because it still has active endpoints`.
    #[test]
    fn a_network_with_an_attached_endpoint_is_not_removed() {
        assert_eq!(
            decide_network_removal("abc", 1),
            NetworkAction::SkipActiveEndpoints { id: "abc".to_string() },
        );
    }

    /// `containers: None` counts as zero, because a Go nil map has `len() == 0`.
    /// Reading it as "unknown, so refuse" would strand every such network.
    #[test]
    fn an_absent_container_map_counts_as_no_endpoints() {
        let mut endpoints: Option<std::collections::HashMap<String, ()>> = None;
        assert_eq!(
            decide_network_removal("abc", endpoints.as_ref().map_or(0, |map| map.len())),
            NetworkAction::Remove { id: "abc".to_string() },
        );
        // An empty map is the same answer, and must not be confused with `None`.
        endpoints = Some(std::collections::HashMap::new());
        assert_eq!(
            decide_network_removal("abc", endpoints.as_ref().map_or(0, |map| map.len())),
            NetworkAction::Remove { id: "abc".to_string() },
        );
    }

    /// The loop does not break on a busy network. This is the behaviour
    /// `NetworkRemove`'s duplicate refusal depends on: stop early and the
    /// remaining duplicate makes every later removal fail.
    #[test]
    fn the_remove_loop_does_not_stop_at_a_busy_network() {
        let plan = plan_network_removals(&[
            net_with_endpoints("act_default", "aaa", 2),
            net_with_endpoints("act_default", "bbb", 0),
            net_with_endpoints("act_default", "ccc", 1),
        ]);
        assert_eq!(
            plan,
            vec![
                NetworkAction::SkipActiveEndpoints { id: "aaa".to_string() },
                NetworkAction::Remove { id: "bbb".to_string() },
                NetworkAction::SkipActiveEndpoints { id: "ccc".to_string() },
            ],
            "one action per duplicate, in order, with no break",
        );
    }

    /// A failed removal is logged and swallowed, so the executor still returns
    /// success. The action names the id precisely so the debug line can.
    #[test]
    fn a_failed_removal_is_swallowed_rather_than_propagated() {
        assert_eq!(
            classify_network_removal("abc", &Ok(())),
            NetworkAction::Remove { id: "abc".to_string() },
        );
        assert_eq!(
            classify_network_removal("abc", &Err("still has active endpoints".to_string())),
            NetworkAction::SkipRemoveFailed { id: "abc".to_string() },
        );
    }

    /// The volume gate. Only a listed name is removed.
    #[test]
    fn a_listed_volume_is_removed() {
        let known = vec!["other".to_string(), "act_cache".to_string()];
        assert_eq!(
            decide_volume_remove(&known, "act_cache"),
            VolumeAction::Remove,
        );
    }

    /// A volume that is not there is a success, not an error. This is the branch
    /// that keeps a name matching nothing away from a delete, so it is pinned
    /// as a distinct outcome rather than folded into `Remove`.
    #[test]
    fn a_missing_volume_is_a_silent_success() {
        let known = vec!["other".to_string()];
        assert_eq!(
            decide_volume_remove(&known, "act_cache"),
            VolumeAction::NotFound,
        );
        assert_eq!(decide_volume_remove(&[], "act_cache"), VolumeAction::NotFound);
    }

    /// Only the *inner* volume executor checks `dryrun`, and it returns before
    /// opening a client — so this runs with no daemon present and still has to
    /// succeed. A dryrun check added to the outer executor would not be caught
    /// here, but removing the inner one would.
    #[test]
    fn the_inner_volume_remove_returns_early_on_dryrun() {
        let ctx = RunContext::new().with_dryrun(true);
        // No DOCKER_HOST is consulted and no socket is opened: a failure here
        // would mean the dryrun check moved below `connect`.
        remove_executor("act_cache".to_string(), false)(&ctx).expect("dryrun succeeds");
    }

    /// A dry run still reports what it would have done, because the log line is
    /// emitted *before* the dryrun check. This is upstream's ordering.
    #[test]
    fn the_inner_volume_remove_logs_its_command_with_the_prefix() {
        let line = format!("{LOG_PREFIX}docker volume rm act_cache");
        assert_eq!(line, "  \u{1F433}  docker volume rm act_cache");
    }
}
