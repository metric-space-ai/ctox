// ref: internal/watcher/watcher_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::config_reload::{JsonConfigDecoder, WatcherConfig};
use super::dispatcher::{AuthUpdateDispatcher, AuthUpdateSink};
use super::events::{match_provider, ChannelEventSource, FsEvent, FsEventKind};
use super::{
    ConfigReloadSink, NativeWatchFilesystem, NoopPersistenceSink, SystemWatchClock, Watcher,
    WatcherDependencies, WatcherError,
};
use std::io;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

#[derive(Default)]
struct CapturingUpdateSink(Mutex<Vec<super::AuthUpdate>>);
impl AuthUpdateSink for CapturingUpdateSink {
    fn send_batch(&self, updates: &[super::AuthUpdate]) -> io::Result<()> {
        self.0.lock().unwrap().extend_from_slice(updates);
        Ok(())
    }
}
#[derive(Default)]
struct CapturingReloadSink(Mutex<Vec<WatcherConfig>>);
impl ConfigReloadSink for CapturingReloadSink {
    fn on_reload(&self, config: &WatcherConfig) {
        self.0.lock().unwrap().push(config.clone());
    }
}

fn dependencies(
    events: Arc<ChannelEventSource>,
    updates: Arc<CapturingUpdateSink>,
    reloads: Arc<CapturingReloadSink>,
) -> WatcherDependencies {
    WatcherDependencies {
        filesystem: Arc::new(NativeWatchFilesystem),
        clock: Arc::new(SystemWatchClock),
        events,
        config_decoder: Arc::new(JsonConfigDecoder),
        reload_sink: reloads,
        persistence_sink: Arc::new(NoopPersistenceSink),
        dispatcher: Arc::new(AuthUpdateDispatcher::start(8, updates)),
        plugin_parser: None,
        coalesce_window: Duration::ZERO,
        remove_debounce: Duration::from_millis(50),
    }
}
