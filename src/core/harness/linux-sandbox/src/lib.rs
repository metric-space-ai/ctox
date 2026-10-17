//! Linux sandbox helper entry point.
//!
//! On Linux, `ctox-linux-sandbox` applies:
//! - in-process restrictions (`no_new_privs` + seccomp), and
//! - bubblewrap for filesystem isolation.
#[cfg(target_os = "linux")]
mod bwrap;
#[cfg(target_os = "linux")]
mod landlock;
#[cfg(target_os = "linux")]
mod linux_run_main;
#[cfg(target_os = "linux")]
mod proxy_routing;
#[cfg(target_os = "linux")]
mod vendored_bwrap;

#[cfg(target_os = "linux")]
pub use proxy_routing::{activate_proxy_routes_in_netns, prepare_host_proxy_route_spec};

/// Apply the existing Harness proxy-only filter after creating the netns bridge.
#[cfg(target_os = "linux")]
pub fn apply_process_network_filter() -> std::io::Result<()> {
    landlock::apply_sandbox_policy_to_current_thread(
        &ctox_protocol::protocol::SandboxPolicy::DangerFullAccess,
        ctox_protocol::protocol::NetworkSandboxPolicy::Enabled,
        std::path::Path::new("/"),
        false,
        true,
        true,
    )
    .map_err(std::io::Error::other)
}
#[cfg(target_os = "linux")]
pub fn run_main() -> ! {
    linux_run_main::run_main();
}

#[cfg(not(target_os = "linux"))]
pub fn run_main() -> ! {
    panic!("ctox-linux-sandbox is only supported on Linux");
}
