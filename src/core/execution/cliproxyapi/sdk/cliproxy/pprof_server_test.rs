// ref: sdk/cliproxy/pprof_server_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::pprof_server::*;

struct Listener {
    stops: AtomicUsize,
}

impl Listener {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            stops: AtomicUsize::new(0),
        })
    }
}

impl PprofListener for Listener {
    fn serve(self: Arc<Self>) -> ListenerFuture {
        Box::pin(std::future::pending())
    }
    fn shutdown(&self, _timeout: Duration) -> ListenerFuture {
        self.stops.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(()) })
    }
}

struct Factory;
impl PprofListenerFactory for Factory {
    fn bind(
        &self,
        _addr: &str,
        _routes: &'static [&'static str],
    ) -> Result<Arc<dyn PprofListener>, PprofError> {
        Ok(Listener::new())
    }
}

fn pprof() -> PprofServer {
    PprofServer::new(Arc::new(Factory), Duration::from_secs(5))
}
