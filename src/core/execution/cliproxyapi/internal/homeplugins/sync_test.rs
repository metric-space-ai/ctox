// ref: internal/homeplugins/sync_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, TimeZone, Utc};
use tempfile::TempDir;

use super::*;

struct FixedBoundary {
    now: DateTime<Utc>,
    reject_tilde: bool,
}

impl FixedBoundary {
    fn new() -> Self {
        Self {
            now: Utc.with_ymd_and_hms(2026, 8, 4, 10, 0, 0).unwrap(),
            reject_tilde: true,
        }
    }
}

impl HomePluginBoundary for FixedBoundary {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }

    fn resolve_plugins_dir(&self, configured: &str) -> Result<PathBuf, SyncError> {
        if self.reject_tilde && configured.trim().starts_with('~') {
            Err(SyncError::new(
                "resolve plugins directory: home directory is unavailable",
            ))
        } else {
            Ok(PathBuf::from(configured))
        }
    }
}

#[derive(Default)]
struct FakeStore {
    fail: Mutex<Option<String>>,
    skipped: AtomicBool,
    loaded_seen: AtomicBool,
    auth_seen: Mutex<Vec<u8>>,
}

#[derive(Default)]
struct FakeSdkIo {
    auth_seen: Mutex<Vec<u8>>,
    loaded_seen: AtomicBool,
    calls: Mutex<usize>,
}

impl crate::sdk::pluginstore::PluginStoreIo for FakeSdkIo {
    fn fetch_registry(
        &self,
        _client: &crate::sdk::pluginstore::Client,
    ) -> crate::sdk::pluginstore::Result<crate::sdk::pluginstore::Registry> {
        unreachable!("sync install must not fetch the registry")
    }

    fn fetch_latest_release(
        &self,
        _client: &crate::sdk::pluginstore::Client,
        _plugin: &crate::sdk::pluginstore::Plugin,
    ) -> crate::sdk::pluginstore::Result<crate::sdk::pluginstore::Release> {
        unreachable!("sync install must not resolve latest")
    }

    fn fetch_release_by_tag(
        &self,
        _client: &crate::sdk::pluginstore::Client,
        _plugin: &crate::sdk::pluginstore::Plugin,
        _tag: &str,
    ) -> crate::sdk::pluginstore::Result<crate::sdk::pluginstore::Release> {
        unreachable!("sync install must not resolve a tag in this fixture")
    }

    fn install(
        &self,
        _client: &crate::sdk::pluginstore::Client,
        _plugin: &crate::sdk::pluginstore::Plugin,
        _options: &crate::sdk::pluginstore::InstallOptions,
    ) -> crate::sdk::pluginstore::Result<InstallResult> {
        unreachable!("sync uses pinned manifest installation")
    }

    fn install_version(
        &self,
        _client: &crate::sdk::pluginstore::Client,
        _plugin: &crate::sdk::pluginstore::Plugin,
        _release_tag: &str,
        _version: &str,
        _options: &crate::sdk::pluginstore::InstallOptions,
    ) -> crate::sdk::pluginstore::Result<InstallResult> {
        unreachable!("sync uses pinned manifest installation")
    }

    fn install_manifest(
        &self,
        client: &crate::sdk::pluginstore::Client,
        manifest: &Manifest,
        options: &crate::sdk::pluginstore::InstallOptions,
    ) -> crate::sdk::pluginstore::Result<InstallResult> {
        *self.calls.lock().unwrap() += 1;
        *self.auth_seen.lock().unwrap() = client.resolved_auth()[0].token.expose().to_vec();
        self.loaded_seen.store(
            options
                .plugin_loaded
                .as_ref()
                .is_some_and(|loaded| loaded()),
            Ordering::SeqCst,
        );
        let path = plugin_path(
            &options.plugins_dir,
            &options.goos,
            &options.goarch,
            &manifest.id,
            &manifest.version,
        );
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"sdk-store-io").unwrap();
        Ok(InstallResult {
            id: manifest.id.clone(),
            version: manifest.version.clone(),
            path,
            ..InstallResult::default()
        })
    }
}

impl PluginStore for FakeStore {
    fn install_manifest(
        &self,
        context: &dyn OperationContext,
        request: InstallRequest<'_>,
    ) -> Result<InstallResult, SyncError> {
        if context.is_cancelled() {
            return Err(SyncError::new("context canceled"));
        }
        if let Some(message) = self.fail.lock().unwrap().clone() {
            return Err(SyncError::new(message));
        }
        self.loaded_seen
            .store(request.plugin_loaded, Ordering::SeqCst);
        if let Some(auth) = request.resolved_auth.first() {
            *self.auth_seen.lock().unwrap() = auth.token.expose().to_vec();
        }
        let path = plugin_path(
            request.plugins_dir,
            &request.platform.goos,
            &request.platform.goarch,
            &request.manifest.id,
            &request.manifest.version,
        );
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let skipped = self.skipped.load(Ordering::SeqCst);
        if !skipped {
            fs::write(&path, b"library-data").unwrap();
        }
        let overwritten = path.exists() && !skipped;
        Ok(InstallResult {
            path,
            skipped,
            overwritten,
            ..InstallResult::default()
        })
    }
}

#[derive(Default)]
struct FakeRuntime {
    busy: AtomicBool,
    unloads: Mutex<Vec<String>>,
    context_cancelled_at_unload: AtomicBool,
}

impl PluginRuntime for FakeRuntime {
    fn plugin_busy(&self, _id: &str) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    fn unload_plugin(&self, context: &dyn OperationContext, id: &str) -> bool {
        self.context_cancelled_at_unload
            .store(context.is_cancelled(), Ordering::SeqCst);
        self.unloads.lock().unwrap().push(id.to_owned());
        self.busy.store(false, Ordering::SeqCst);
        true
    }
}

struct CancelContext(AtomicBool);

impl CancelContext {
    fn cancelled() -> Self {
        Self(AtomicBool::new(true))
    }
}

impl OperationContext for CancelContext {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

struct Inspector(HashMap<String, bool>);

impl PluginLoadInspector for Inspector {
    fn plugin_registered(&self, id: &str) -> bool {
        self.0.get(id).copied().unwrap_or(false)
    }
}

fn manifest(id: &str, version: &str) -> Manifest {
    Manifest {
        id: id.to_owned(),
        name: "Sample".to_owned(),
        description: "Adds sample support.".to_owned(),
        author: "owner".to_owned(),
        version: version.to_owned(),
        release_tag: format!("v{version}"),
        repository: "https://github.com/owner/sample-plugin".to_owned(),
        ..Manifest::default()
    }
}

fn config(root: &Path) -> SyncConfig {
    SyncConfig {
        home_enabled: true,
        plugins_enabled: true,
        plugins_dir: root.to_string_lossy().into_owned(),
        plugins: BTreeMap::from([(
            "sample".to_owned(),
            PluginInstanceConfig {
                enabled: Some(true),
                store: Some(manifest("sample", "0.2.0")),
            },
        )]),
    }
}

fn plugin_path(root: &Path, goos: &str, goarch: &str, id: &str, version: &str) -> PathBuf {
    let name = if version.trim().is_empty() {
        id.trim().to_owned()
    } else {
        format!("{}-v{}", id.trim(), version.trim())
    };
    root.join(goos)
        .join(goarch)
        .join(format!("{name}{}", plugin_extension(goos)))
}
