// Origin: CTOX
// License: AGPL-3.0-only

#![cfg(any(unix, windows))]

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use ctox_cliproxyapi::internal::pluginhost::process_transport::RequestMode;
use ctox_cliproxyapi::internal::pluginhost::supervisor::{
    PluginProcessConfig, PluginSupervisor, PluginSupervisorError, RestartPolicy,
};

fn runtime_root(label: &str) -> PathBuf {
    #[cfg(unix)]
    let base = PathBuf::from("/tmp");
    #[cfg(windows)]
    let base = std::env::temp_dir();
    let root = base.join(format!("cpa-supervisor-{label}-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    root
}

fn config(root: PathBuf, plugin_id: &str, max_restarts: u32) -> PluginProcessConfig {
    PluginProcessConfig {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_cliproxy-differential")),
        runtime_root: root,
        instance_id: "fixture".into(),
        plugin_id: plugin_id.into(),
        restart_policy: RestartPolicy {
            max_restarts,
            base_delay: Duration::from_millis(20),
            max_delay: Duration::from_millis(40),
        },
    }
}
