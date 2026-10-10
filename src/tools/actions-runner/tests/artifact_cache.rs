//! The upstream acceptance tests for `pkg/artifactcache`.
//!
//! All five upstream test functions are ported, and every subtest of
//! `TestHandler` with it. act drives its tests through `net/http` over a real
//! socket, so this does too — `Handler::serve` runs on a background thread and
//! [`http_request`] speaks HTTP/1.1 straight to the port. The client is a
//! hundred lines rather than a dependency, which keeps the crate's dependency
//! list to what the service itself needs.
//!
//! Two upstream details are worked around rather than reproduced:
//!
//! * `TestHandler` sleeps a second between three reservations so their
//!   `CreatedAt` values differ, because the prefix lookup orders by
//!   `CreatedAt` and a tie has no defined winner. Here the timestamps are
//!   written directly, which is what the sleep was arranging, without the
//!   three seconds it costs.
//! * `TestHandler_CustomExternalURL` assigns to an unexported field. The port
//!   exposes [`Handler::with_custom_external_url`] for the same purpose.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use ctox_actions_runner::artifactcache::{Call, Cache, Handler};

const VERSION: &str = "c19da02a2bd7e77277f1ac29ab45c09b7d46a4ee758284e26bb3045ad11d9d20";

/// A response, as read off the wire.
struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

impl HttpResponse {
    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

/// One HTTP/1.1 request over a socket, `Connection: close` in and out.
fn http_request(
    port: u16,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> std::io::Result<HttpResponse> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    let mut request = format!(
        "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| std::io::Error::other("no header terminator in the response"))?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| std::io::Error::other("no status code in the response"))?;
    Ok(HttpResponse {
        status,
        body: body.as_bytes().to_vec(),
    })
}


/// ---------------------------------------------------------------------------
// file_collector-style helper: a body of `len` deterministic bytes.
/// ---------------------------------------------------------------------------
fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

// ---------------------------------------------------------------------------
// handler_test.go: TestHandler
// ---------------------------------------------------------------------------

#[test]
fn handler() {
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("artifactcache");
    let mut handler = Handler::start(&cache_dir, "", "127.0.0.1", 0).unwrap();
    let port = handler.actual_port();
    // The closures below capture their own copy, so `port` itself stays
    // available for the free helper functions.
    let wire = port;
    let token = handler.token().to_string();
    let base = format!("/{token}/_apis/artifactcache");
    std::thread::spawn(move || handler.serve());

    let get = |target: &str| {
        http_request(wire, "GET", target, &[], b"").expect("the request completes")
    };
    let post = |target: &str, body: &[u8]| {
        http_request(wire, "POST", target, &[], body).expect("the request completes")
    };
    let patch = |target: &str, range: &str, body: &[u8]| {
        http_request(
            wire,
            "PATCH",
            target,
            &[
                ("Content-Type", "application/octet-stream"),
                ("Content-Range", range),
            ],
            body,
        )
        .expect("the request completes")
    };

    // "get not exist"
    let response = get(&format!("{base}/cache?keys=key&version={VERSION}"));
    assert_eq!(response.status, 204);

    // "reserve and upload"
    {
        let key = "reserve_and_upload";
        let content = payload(100);
        let id = reserve(wire, &base, key, VERSION, content.len() as i64);
        assert_eq!(patch(&format!("{base}/caches/{id}"), "bytes 0-99/*", &content).status, 200);
        assert_eq!(post(&format!("{base}/caches/{id}"), b"").status, 200);
    }

    // "clean"
    assert_eq!(post(&format!("{base}/clean"), b"").status, 200);

    // "reserve with bad request"
    {
        let response = post(&format!("{base}/caches"), b"invalid json");
        assert_eq!(response.status, 400);
    }

    // "duplicate reserve" — two reservations of the same key get distinct ids
    {
        let first = reserve(wire, &base, "duplicate_reserve", VERSION, 100);
        let second = reserve(wire, &base, "duplicate_reserve", VERSION, 100);
        assert_ne!(first, 0);
        assert_ne!(second, 0);
        assert_ne!(first, second);
    }

    // "upload with bad id"
    assert_eq!(patch(&format!("{base}/caches/invalid_id"), "bytes 0-99/*", b"").status, 400);

    // "upload without reserve"
    assert_eq!(patch(&format!("{base}/caches/1000"), "bytes 0-99/*", b"").status, 400);

    // "upload with complete"
    {
        let key = "upload_with_complete";
        let content = payload(100);
        let id = reserve(wire, &base, key, VERSION, 100);
        assert_eq!(patch(&format!("{base}/caches/{id}"), "bytes 0-99/*", &content).status, 200);
        assert_eq!(post(&format!("{base}/caches/{id}"), b"").status, 200);
        // A second upload of a completed entry is refused.
        assert_eq!(patch(&format!("{base}/caches/{id}"), "bytes 0-99/*", &content).status, 400);
    }

    // "upload with invalid range"
    {
        let key = "upload_with_invalid_range";
        let id = reserve(wire, &base, key, VERSION, 100);
        let response = patch(&format!("{base}/caches/{id}"), "bytes xx-99/*", &payload(100));
        assert_eq!(response.status, 400);
    }

    // "commit with bad id"
    assert_eq!(post(&format!("{base}/caches/invalid_id"), b"").status, 400);

    // "commit with not exist id"
    assert_eq!(post(&format!("{base}/caches/100"), b"").status, 400);

    // "duplicate commit"
    {
        let key = "duplicate_commit";
        let content = payload(100);
        let id = reserve(wire, &base, key, VERSION, 100);
        assert_eq!(patch(&format!("{base}/caches/{id}"), "bytes 0-99/*", &content).status, 200);
        assert_eq!(post(&format!("{base}/caches/{id}"), b"").status, 200);
        assert_eq!(post(&format!("{base}/caches/{id}"), b"").status, 400);
    }

    // "commit early" — a short upload fails the length check at commit time
    {
        let key = "commit_early";
        let id = reserve(wire, &base, key, VERSION, 100);
        let content = payload(100);
        assert_eq!(
            patch(&format!("{base}/caches/{id}"), "bytes 0-59/*", &content[..50]).status,
            200
        );
        assert_eq!(post(&format!("{base}/caches/{id}"), b"").status, 500);
    }

    // "get with bad id"
    assert_eq!(get(&format!("{base}/artifacts/invalid_id")).status, 400);

    // "get with not exist id"
    assert_eq!(get(&format!("{base}/artifacts/100")).status, 404);
    assert_eq!(get(&format!("{base}/artifacts/100")).status, 404);

    // "case insensitive"
    {
        let key = "case_insensitive";
        let content = payload(100);
        // Stored upper, requested mixed; both fold to lower case.
        upload(wire, &base, &format!("{key}_ABC"), VERSION, &content);
        let response = get(&format!("{base}/cache?keys={key}_aBc&version={VERSION}"));
        assert_eq!(response.status, 200);
        let body = response.json();
        assert_eq!(body["result"], "hit");
        assert_eq!(body["cacheKey"], format!("{key}_abc"));
    }
}

/// `uploadCacheNormally`, over a socket.
fn upload(port: u16, base: &str, key: &str, version: &str, content: &[u8]) {
    let id = reserve(port, base, key, version, content.len() as i64);
    let response = http_request(
        port,
        "PATCH",
        &format!("{base}/caches/{id}"),
        &[
            ("Content-Type", "application/octet-stream"),
            ("Content-Range", "bytes 0-99/*"),
        ],
        content,
    )
    .unwrap();
    assert_eq!(response.status, 200);
    let response = http_request(port, "POST", &format!("{base}/caches/{id}"), &[], b"").unwrap();
    assert_eq!(response.status, 200);

    let response =
        http_request(port, "GET", &format!("{base}/cache?keys={key}&version={version}"), &[], b"")
            .unwrap();
    assert_eq!(response.status, 200);
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["result"], "hit");
    assert_eq!(body["cacheKey"], key.to_lowercase());

    let location = body["archiveLocation"].as_str().unwrap();
    let target = location.split_once(base).expect("the base is a prefix").1;
    let downloaded = http_request(port, "GET", &format!("{base}{target}"), &[], b"").unwrap();
    assert_eq!(downloaded.status, 200);
    assert_eq!(downloaded.body, content);
}

/// `POST /caches` and read back the id.
fn reserve(port: u16, base: &str, key: &str, version: &str, size: i64) -> u64 {
    let body = serde_json::json!({ "key": key, "version": version, "cacheSize": size });
    let response = http_request(
        port,
        "POST",
        &format!("{base}/caches"),
        &[("Content-Type", "application/json")],
        body.to_string().as_bytes(),
    )
    .unwrap();
    assert_eq!(response.status, 200);
    let parsed: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    let id = parsed["cacheId"].as_u64().expect("a cacheId is returned");
    assert_ne!(id, 0);
    id
}

// ---------------------------------------------------------------------------
// handler_test.go: the ordering subtests
// ---------------------------------------------------------------------------

/// The three ordering subtests need `CreatedAt` values that differ, which
/// upstream arranges with a one-second sleep. Timestamps are written directly
/// here instead, which is what the sleep was arranging.
#[test]
fn handler_lookup_ordering() {
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("artifactcache");
    let mut handler = Handler::start(&cache_dir, "", "127.0.0.1", 0).unwrap();
    let port = handler.actual_port();
    let base = format!("/{}/_apis/artifactcache", handler.token());
    let service = handler.service();
    std::thread::spawn(move || handler.serve());

    // "get with multiple keys": `key_a_b_x` matches nothing. `key_a_b` matches
    // both `key_a_b` and `key_a_b_c`, and the upload order puts `key_a_b`
    // second, so it is the newer of the two and wins. The answer is therefore
    // the *second* requested key, not the third.
    {
        let key = "get_with_multiple_keys";
        let keys = [format!("{key}_a_b_c"), format!("{key}_a_b"), format!("{key}_a")];
        let contents: Vec<Vec<u8>> = keys.iter().map(|_| payload(100)).collect();
        for (i, (key, content)) in keys.iter().zip(contents.iter()).enumerate() {
            let id = reserve(port, &base, key, VERSION, content.len() as i64);
            upload_at(&service, id, i as i64);
            let response = http_request(
                port,
                "PATCH",
                &format!("{base}/caches/{id}"),
                &[
                    ("Content-Type", "application/octet-stream"),
                    ("Content-Range", "bytes 0-99/*"),
                ],
                content,
            )
            .unwrap();
            assert_eq!(response.status, 200);
            assert_eq!(
                http_request(port, "POST", &format!("{base}/caches/{id}"), &[], b"").unwrap().status,
                200
            );
        }

        let requested = format!("{key}_a_b_x,{key}_a_b,{key}_a");
        let response = http_request(
            port,
            "GET",
            &format!("{base}/cache?keys={requested}&version={VERSION}"),
            &[],
            b"",
        )
        .unwrap();
        assert_eq!(response.status, 200);
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["result"], "hit");
        // `key_a_b` and `key_a_b_c` both match the second requested key, and
        // `key_a_b` was created later, so it is the one returned.
        assert_eq!(body["cacheKey"], format!("{key}_a_b"));

        let location = body["archiveLocation"].as_str().unwrap();
        let target = location.split_once(&base).unwrap().1;
        let downloaded = http_request(port, "GET", &format!("{base}{target}"), &[], b"").unwrap();
        assert_eq!(downloaded.status, 200);
        assert_eq!(
            downloaded.body, contents[1],
            "the payload of the returned key, not of the oldest match"
        );
    }

    // "exact keys are preferred (key 0)" and "(key 1)": an exact hit beats a
    // prefix hit even when a later requested key would also match.
    for (label, requested_keys) in [
        ("key 0", vec!["_a", "_a_b"]),
        ("key 1", vec!["------------------------------------------------------", "_a", "_a_b"]),
    ] {
        let key = format!("exact_keys_are_preferred_{}", label.replace(' ', "_"));
        let keys = [
            format!("{key}_a"),
            format!("{key}_a_b_c"),
            format!("{key}_a_b"),
        ];
        for (i, cache_key) in keys.iter().enumerate() {
            let id = reserve(port, &base, cache_key, VERSION, 100);
            upload_at(&service, id, i as i64);
            let response = http_request(
                port,
                "PATCH",
                &format!("{base}/caches/{id}"),
                &[
                    ("Content-Type", "application/octet-stream"),
                    ("Content-Range", "bytes 0-99/*"),
                ],
                &payload(100),
            )
            .unwrap();
            assert_eq!(response.status, 200);
            assert_eq!(
                http_request(port, "POST", &format!("{base}/caches/{id}"), &[], b"").unwrap().status,
                200
            );
        }

        let requested: Vec<String> = requested_keys
            .iter()
            .map(|suffix| format!("{key}{suffix}"))
            .collect();
        let response = http_request(
            port,
            "GET",
            &format!("{base}/cache?keys={}&version={VERSION}", requested.join(",")),
            &[],
            b"",
        )
        .unwrap();
        assert_eq!(response.status, 200);
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(
            body["cacheKey"],
            keys[0],
            "{label}: an exact match wins over a prefix match"
        );
    }
}

/// Backdates a reservation so several entries have distinct `CreatedAt` values.
fn upload_at(
    service: &Arc<ctox_actions_runner::artifactcache::Service>,
    id: u64,
    created_at: i64,
) {
    let mut cache = service.database().get(id).unwrap().expect("the reservation exists");
    cache.created_at = created_at;
    cache.used_at = created_at;
    service.database().put(&cache).unwrap();
}

// ---------------------------------------------------------------------------
// handler_test.go: TestHandler_gcCache
// ---------------------------------------------------------------------------

#[test]
fn handler_gc_cache() {
    use ctox_actions_runner::artifactcache::{KEEP_OLD, KEEP_TEMP, KEEP_UNUSED, KEEP_USED};

    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().join("artifactcache");
    let handler = Handler::start(&cache_dir, "", "127.0.0.1", 0).unwrap();
    let service = handler.service();

    let now = ctox_actions_runner::artifactcache::unix_now();
    let cases: Vec<(Cache, bool)> = vec![
        (
            Cache {
                key: "test_key_1".into(),
                version: "test_version".into(),
                complete: true,
                used_at: now,
                created_at: now - 3600,
                ..Cache::default()
            },
            true, // used recently and not too old
        ),
        (
            Cache {
                key: "test_key_2".into(),
                version: "test_version".into(),
                complete: false,
                used_at: now - (KEEP_TEMP.as_secs() as i64 + 1),
                created_at: now - (KEEP_TEMP.as_secs() as i64 + 3600),
                ..Cache::default()
            },
            false, // unfinished and abandoned
        ),
        (
            Cache {
                key: "test_key_3".into(),
                version: "test_version".into(),
                complete: true,
                used_at: now - (KEEP_UNUSED.as_secs() as i64 + 1),
                created_at: now - (KEEP_UNUSED.as_secs() as i64 + 3600),
                ..Cache::default()
            },
            false, // unused for a while
        ),
        (
            Cache {
                key: "test_key_4".into(),
                version: "test_version".into(),
                complete: true,
                used_at: now,
                created_at: now - (KEEP_USED.as_secs() as i64 + 1),
                ..Cache::default()
            },
            false, // used but too old
        ),
        (
            Cache {
                key: "test_key_1".into(),
                version: "test_version".into(),
                complete: true,
                used_at: now - (KEEP_OLD.as_secs() as i64 - 60),
                created_at: now - 3601,
                ..Cache::default()
            },
            true, // superseded but used recently
        ),
        (
            Cache {
                key: "test_key_1".into(),
                version: "test_version".into(),
                complete: true,
                used_at: now - (KEEP_OLD.as_secs() as i64 + 1),
                created_at: now - 3601,
                ..Cache::default()
            },
            false, // superseded and not used recently
        ),
    ];

    let mut inserted = Vec::new();
    for (cache, _) in &cases {
        let mut cache = cache.clone();
        service.database().insert(&mut cache).unwrap();
        inserted.push(cache);
    }

    // act's constructor collects once, so the second collection would be
    // skipped by the one-hour window. This is that second collection.
    service.gc_forced();

    for (index, (cache, kept)) in cases.iter().enumerate() {
        let found = service.database().get(inserted[index].id).unwrap();
        if *kept {
            assert!(found.is_some(), "case {index} ({}) should be kept", cache.key);
        } else {
            assert!(found.is_none(), "case {index} ({}) should be gone", cache.key);
        }
    }
}

// ---------------------------------------------------------------------------
// handler_test.go: TestHandler_UnauthorizedAccess
// ---------------------------------------------------------------------------

#[test]
fn unauthorized_access_is_a_bare_404() {
    let dir = tempfile::tempdir().unwrap();
    let mut handler = Handler::start(&dir.path().join("artifactcache"), "", "127.0.0.1", 0).unwrap();
    let port = handler.actual_port();
    std::thread::spawn(move || handler.serve());

    // Without the token segment nothing routes.
    let base = "/_apis/artifactcache";
    let response = http_request(
        port,
        "GET",
        &format!("{base}/cache?keys=test&version=abc"),
        &[],
        b"",
    )
    .unwrap();
    assert_eq!(response.status, 404);

    let response = http_request(
        port,
        "POST",
        &format!("{base}/caches"),
        &[("Content-Type", "application/json")],
        b"{}",
    )
    .unwrap();
    assert_eq!(response.status, 404);
}

// ---------------------------------------------------------------------------
// handler_test.go: TestHandler_BindAddress and TestHandler_CustomExternalURL
// ---------------------------------------------------------------------------

#[test]
fn bind_address_is_the_one_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let handler = Handler::start(&dir.path().join("artifactcache"), "", "127.0.0.1", 0).unwrap();
    assert!(
        handler.bind_address().starts_with("127.0.0.1:"),
        "got {}",
        handler.bind_address()
    );
    assert_eq!(handler.outbound_ip(), "127.0.0.1");
    assert_ne!(handler.actual_port(), 0, "port 0 resolves to a real port");
}

#[test]
fn custom_external_url_wins() {
    let dir = tempfile::tempdir().unwrap();
    let mut handler = Handler::start(&dir.path().join("artifactcache"), "", "127.0.0.1", 0).unwrap();
    let port = handler.actual_port();
    let token = handler.token().to_string();
    let base = format!("/{token}/_apis/artifactcache");

    // The advertised URL is derived from the bound port.
    assert_eq!(
        handler.with_custom_external_url(&format!("http://127.0.0.1:{port}")),
        format!("http://127.0.0.1:{port}/{token}")
    );

    // "advertise url set wrong" — act does not validate the base, it only
    // appends the token.
    assert_eq!(
        handler.with_custom_external_url("http://127.0.0.999:1234"),
        format!("http://127.0.0.999:1234/{token}")
    );

    std::thread::spawn(move || handler.serve());

    // The service still works on the port it actually bound.
    let response =
        http_request(port, "POST", &format!("{base}/caches"), &[], b"{\"key\":\"k\",\"version\":\"v\",\"cacheSize\":1}").unwrap();
    assert_eq!(response.status, 200);
}

// ---------------------------------------------------------------------------
// Closing the service
// ---------------------------------------------------------------------------

#[test]
fn closed_server_is_unreachable() {
    let dir = tempfile::tempdir().unwrap();
    let mut handler = Handler::start(&dir.path().join("artifactcache"), "", "127.0.0.1", 0).unwrap();
    let port = handler.actual_port();
    handler.close();
    // act's subtest posts to a closed server and expects an error.
    assert!(
        TcpStream::connect(("127.0.0.1", port)).is_err(),
        "the listener must be released"
    );
}

// ---------------------------------------------------------------------------
// The routing, exercised without a socket
// ---------------------------------------------------------------------------

#[test]
fn routing_without_a_socket() {
    let dir = tempfile::tempdir().unwrap();
    let handler = Handler::start(&dir.path().join("artifactcache"), "", "127.0.0.1", 0).unwrap();
    let service = handler.service();
    let token = handler.token().to_string();

    let routed = |call: Call| service.route(&token, &call);

    // No token, no route.
    assert!(service
        .route("wrong", &Call::get("/_apis/artifactcache/cache?keys=k&version=v"))
        .is_none());
    // A token but an unknown path.
    assert!(routed(Call::get(&format!("/{token}/_apis/artifactcache/nope"))).is_none());
    // A subpath of a known route is a different path.
    assert!(routed(Call::get(&format!("/{token}/_apis/artifactcache/cache/extra"))).is_none());

    // A miss is 204.
    let reply = routed(Call::get(&format!(
        "/{token}/_apis/artifactcache/cache?keys=absent&version={VERSION}"
    )))
    .expect("the route exists");
    assert_eq!(reply.status, 204);
    assert_eq!(reply.body, b"{}");

    // The right path with the wrong method is a 405, as httprouter answers.
    assert_eq!(
        routed(Call::new(
            "POST",
            &format!("/{token}/_apis/artifactcache/cache?keys=k&version=v"),
            b""
        ))
        .expect("the path exists")
        .status,
        405
    );

    // Reserve, upload, commit and find, all in process.
    let id = {
        let body = serde_json::json!({ "key": "routed", "version": VERSION, "cacheSize": 3 });
        let reply = routed(Call::new(
            "POST",
            &format!("/{token}/_apis/artifactcache/caches"),
            body.to_string().as_bytes(),
        ))
        .expect("the route exists");
        assert_eq!(reply.status, 200);
        reply.json_body()["cacheId"].as_u64().expect("a cacheId")
    };
    let uploaded = routed(
        Call::new(
            "PATCH",
            &format!("/{token}/_apis/artifactcache/caches/{id}"),
            b"abc",
        )
        .with_header("Content-Range", "bytes 0-2/*"),
    )
    .expect("the route exists");
    assert_eq!(uploaded.status, 200);
    let committed = routed(Call::new(
        "POST",
        &format!("/{token}/_apis/artifactcache/caches/{id}"),
        b"",
    ))
    .expect("the route exists");
    assert_eq!(committed.status, 200);

    let hit = routed(Call::get(&format!(
        "/{token}/_apis/artifactcache/cache?keys=routed&version={VERSION}"
    )))
    .expect("the route exists");
    assert_eq!(hit.status, 200);
    assert_eq!(hit.json_body()["cacheKey"], "routed");
    assert!(hit.json_body()["archiveLocation"]
        .as_str()
        .unwrap()
        .ends_with(&format!("/_apis/artifactcache/artifacts/{id}")));

    let payload = routed(Call::get(&format!(
        "/{token}/_apis/artifactcache/artifacts/{id}"
    )))
    .expect("the route exists");
    assert_eq!(payload.status, 200);
    assert_eq!(payload.body, b"abc");
}
