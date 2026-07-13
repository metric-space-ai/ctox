//! Issue #21 reproduction as an automated integration test (P0-R1).
//!
//! Contract under test: one dead chat client can lose only its own
//! subscription. It must not poison the service event stream, later clients
//! or server-side work — no daemon restart allowed between turns.
//!
//! The scenario drives the REAL service binary (`ctox service --foreground`)
//! against a mock local Responses runtime (python fixture speaking the
//! NDJSON-over-unix-socket wire plus the /tokenize HTTP preflight), kills a
//! `chat --wait` client mid-turn with SIGKILL and then requires fresh
//! clients to complete turns. The 100-iteration evidence run reuses the same
//! harness and is `#[ignore]`d for normal CI; run it explicitly with
//! `cargo test --test event_stream_disconnect -- --ignored`.
#![cfg(unix)]

#[path = "../harness/mod.rs"]
mod harness;

use harness::TestRoot;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

const MOCK_MODEL: &str = "Qwen/Qwen3.6-27B";

/// Root for this test family. Unix sockets cap at ~100 bytes of path, and
/// the managed-runtime socket lives under `<root>/runtime/sockets/`, so the
/// root itself must stay short — the default per-test roots are too deep.
fn short_test_root(label: &str) -> TestRoot {
    // Literal /tmp, NOT std::env::temp_dir(): macOS resolves temp_dir to a
    // long /var/folders/... path that pushes the managed-runtime socket past
    // the ~100-byte AF_UNIX limit.
    TestRoot::new_at(&PathBuf::from(format!(
        "/tmp/ctox-esd-{label}-{}",
        std::process::id()
    )))
}

struct MockLocalRuntime {
    child: Child,
    log_path: PathBuf,
    delay_path: PathBuf,
}

impl MockLocalRuntime {
    /// Spawn the python mock with the socket path and model tag in argv —
    /// the supervisor adopts a socket-backed backend only when a live
    /// process's ps command line contains both.
    fn start(root: &TestRoot) -> Self {
        let socket_path = root.path("runtime/sockets/primary_generation.sock");
        std::fs::create_dir_all(socket_path.parent().expect("sockets dir"))
            .expect("create sockets dir");
        let log_path = root.path("mock.log");
        let delay_path = root.path("mock-delay");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/mock_local_responses.py");
        let log = std::fs::File::create(&log_path).expect("create mock log");
        let child = std::process::Command::new("python3")
            .arg(fixture)
            .arg(&socket_path)
            .arg(MOCK_MODEL)
            .arg(&delay_path)
            .arg("0")
            .stdout(log.try_clone().expect("clone log handle"))
            .stderr(log)
            .spawn()
            .expect("spawn mock local runtime (python3 required)");
        let mock = Self {
            child,
            log_path,
            delay_path,
        };
        mock.wait_for_listening();
        mock
    }

    fn wait_for_listening(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if std::fs::read_to_string(&self.log_path)
                .unwrap_or_default()
                .contains("LISTENING")
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "mock local runtime never started listening"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn set_delay(&self, delay: Duration) {
        std::fs::write(&self.delay_path, format!("{}", delay.as_secs_f64()))
            .expect("write mock delay");
    }

    /// Number of model requests the mock has served (turns + helper turns).
    fn hit_count(&self) -> usize {
        std::fs::read_to_string(&self.log_path)
            .unwrap_or_default()
            .matches("MOCK HIT")
            .count()
    }
}

impl Drop for MockLocalRuntime {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Seed the isolated test root with a mock LOCAL runtime (candle) so the
/// direct session speaks the socket-based Responses transport to the mock.
/// Uses the same SQLite runtime-env store the daemon reads (isolated
/// test-root configuration).
fn seed_mock_local_runtime(root: &TestRoot) {
    let db_path = root.path("runtime/ctox-runtime.sqlite3");
    std::fs::create_dir_all(db_path.parent().expect("runtime dir")).expect("create runtime dir");
    let conn = rusqlite::Connection::open(&db_path).expect("open runtime env store");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS runtime_env_kv (
             env_key TEXT PRIMARY KEY,
             env_value TEXT NOT NULL
         );",
    )
    .expect("init runtime env table");
    for (key, value) in [
        ("CTOX_CHAT_MODEL", MOCK_MODEL),
        ("CTOX_CHAT_SOURCE", "local"),
        ("CTOX_LOCAL_RUNTIME", "candle"),
        ("CTOX_AUTONOMY_LEVEL", "defensive"),
    ] {
        conn.execute(
            "INSERT INTO runtime_env_kv (env_key, env_value) VALUES (?1, ?2)
             ON CONFLICT(env_key) DO UPDATE SET env_value = excluded.env_value",
            rusqlite::params![key, value],
        )
        .expect("seed runtime env");
    }
}

fn wait_for_service_running(root: &TestRoot, deadline: Duration) {
    let start = Instant::now();
    loop {
        let output = root.run(&["status"]);
        let json = output.success().json();
        if json["running"].as_bool() == Some(true) {
            return;
        }
        assert!(
            start.elapsed() < deadline,
            "service did not reach running=true within {deadline:?}; last status: {json}"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn spawn_chat_client(root: &TestRoot, prompt: &str, thread_key: &str) -> Child {
    std::process::Command::new(env!("CARGO_BIN_EXE_ctox"))
        .args([
            "chat",
            prompt,
            "--thread-key",
            thread_key,
            "--wait",
            "--timeout-secs",
            "300",
        ])
        .env("CTOX_ROOT", root.path(""))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn chat client")
}

fn run_quick_chat(root: &TestRoot, label: &str) {
    let output = root.run(&[
        "chat",
        &format!("quick check {label}"),
        "--thread-key",
        &format!("fresh-{label}"),
        "--wait",
        "--timeout-secs",
        "120",
    ]);
    let json = output.success().json();
    assert_eq!(
        json["status"].as_str(),
        Some("completed"),
        "fresh client {label} must complete: {json}"
    );
}

struct ServiceGuard(Child);

impl Drop for ServiceGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// One full kill cycle: start a long turn, SIGKILL the waiting client once
/// the turn provably reached the mock model, then require a fresh client on
/// a fresh thread to complete without a daemon restart.
fn kill_one_client_and_verify_recovery(
    root: &TestRoot,
    mock: &MockLocalRuntime,
    service: &mut ServiceGuard,
    iteration: usize,
) {
    mock.set_delay(Duration::from_secs(20));
    let hits_before = mock.hit_count();
    let mut victim = spawn_chat_client(
        root,
        &format!("long turn victim {iteration}"),
        &format!("victim-{iteration}"),
    );

    let in_flight_deadline = Instant::now() + Duration::from_secs(120);
    while mock.hit_count() == hits_before {
        assert!(
            Instant::now() < in_flight_deadline,
            "iteration {iteration}: turn never reached the mock model runtime"
        );
        assert!(
            service.0.try_wait().expect("poll service").is_none(),
            "iteration {iteration}: service died before the victim turn started"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    victim.kill().expect("SIGKILL victim chat client");
    let _ = victim.wait();

    mock.set_delay(Duration::ZERO);
    run_quick_chat(root, &format!("iter-{iteration}"));
    assert!(
        service.0.try_wait().expect("poll service").is_none(),
        "iteration {iteration}: service must survive without restart"
    );
}

/// Plan P0-R1: start a long turn, SIGKILL the waiting client mid-turn, then
/// complete new turns from fresh clients without restarting the daemon.
#[test]
fn killed_chat_client_does_not_poison_later_clients() {
    let root = short_test_root("kill");
    seed_mock_local_runtime(&root);
    let mock = MockLocalRuntime::start(&root);

    let mut service = ServiceGuard(root.spawn(&["service", "--foreground"]));
    wait_for_service_running(&root, Duration::from_secs(60));

    kill_one_client_and_verify_recovery(&root, &mock, &mut service, 0);
    // A second and third fresh client after the kill, for good measure.
    for label in ["b", "c"] {
        run_quick_chat(&root, label);
    }
}

/// Plan P0-R1 evidence run: at least 100 abrupt-disconnect iterations with
/// zero poisoned follow-up clients and no daemon restart. `#[ignore]`d for
/// normal CI; run explicitly for release evidence.
#[test]
#[ignore = "release evidence run: cargo test --test event_stream_disconnect -- --ignored"]
fn hundred_abrupt_disconnect_iterations() {
    let root = short_test_root("kill100");
    seed_mock_local_runtime(&root);
    let mock = MockLocalRuntime::start(&root);

    let mut service = ServiceGuard(root.spawn(&["service", "--foreground"]));
    wait_for_service_running(&root, Duration::from_secs(60));

    for iteration in 0..100 {
        kill_one_client_and_verify_recovery(&root, &mock, &mut service, iteration);
    }
}
