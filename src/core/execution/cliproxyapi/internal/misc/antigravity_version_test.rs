// ref: internal/misc/antigravity_version_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use super::antigravity_version::*;

struct Clock(Mutex<SystemTime>);

impl AntigravityVersionClock for Clock {
    fn now(&self) -> SystemTime {
        *self.0.lock().unwrap()
    }
}

struct Transport {
    requests: Mutex<Vec<AntigravityManifestRequest>>,
    response: Mutex<Result<Vec<u8>, AntigravityVersionError>>,
}

impl AntigravityManifestTransport for Transport {
    fn fetch<'a>(&'a self, request: AntigravityManifestRequest) -> AntigravityManifestFuture<'a> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            self.response.lock().unwrap().clone()
        })
    }
}

fn fixture(
    response: Result<Vec<u8>, AntigravityVersionError>,
) -> (Arc<AntigravityVersionCache>, Arc<Transport>, Arc<Clock>) {
    let transport = Arc::new(Transport {
        requests: Mutex::new(Vec::new()),
        response: Mutex::new(response),
    });
    let clock = Arc::new(Clock(Mutex::new(
        SystemTime::UNIX_EPOCH + Duration::from_secs(1),
    )));
    let cache = Arc::new(
        AntigravityVersionCache::new(
            "https://manifest.test/latest.yml",
            transport.clone(),
            clock.clone(),
        )
        .unwrap(),
    );
    (cache, transport, clock)
}

#[tokio::test]
async fn fetches_bounded_hub_manifest_and_caches_version() {
    let (cache, transport, _) = fixture(Ok(
        b"version: 2.2.1\npath: Antigravity-arm64-mac.zip\n".to_vec()
    ));
    assert_eq!(cache.latest_version(), ANTIGRAVITY_FALLBACK_VERSION);
    assert_eq!(cache.refresh().await.unwrap(), "2.2.1");
    assert_eq!(cache.latest_version(), "2.2.1");
    let request = transport.requests.lock().unwrap()[0].clone();
    assert_eq!(request.url, "https://manifest.test/latest.yml");
    assert_eq!(request.user_agent, "electron-builder");
    assert_eq!(request.cache_control, "no-cache");
    assert_eq!(request.timeout, Duration::from_secs(10));
    assert_eq!(request.max_response_bytes, 4096);
    assert!(!format!("{cache:?}").contains("manifest.test"));
}

#[tokio::test]
async fn refresh_failure_keeps_fresh_cache_and_falls_back_after_expiry() {
    let (cache, transport, clock) = fixture(Ok(b"version: 3.4.5\n".to_vec()));
    cache.refresh().await.unwrap();
    *transport.response.lock().unwrap() = Err(AntigravityVersionError::Transport("offline".into()));
    assert!(cache.refresh().await.is_err());
    assert_eq!(cache.latest_version(), "3.4.5");
    *clock.0.lock().unwrap() = SystemTime::UNIX_EPOCH + Duration::from_secs(8 * 60 * 60);
    assert_eq!(cache.latest_version(), ANTIGRAVITY_FALLBACK_VERSION);
    assert!(cache.refresh().await.is_err());
    assert_eq!(cache.latest_version(), ANTIGRAVITY_FALLBACK_VERSION);
}

#[tokio::test]
async fn rejects_invalid_empty_and_oversized_manifest_versions() {
    for body in [
        b"path: file.zip\n".to_vec(),
        b"version: 2.2\n".to_vec(),
        b"version: 2.2.x\n".to_vec(),
        vec![b'x'; 4097],
    ] {
        let (cache, _, _) = fixture(Ok(body));
        assert!(cache.refresh().await.is_err());
        assert_eq!(cache.latest_version(), ANTIGRAVITY_FALLBACK_VERSION);
    }
}
