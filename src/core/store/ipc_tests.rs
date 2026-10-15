use super::*;
#[cfg(unix)]
use crate::{
    registry::Registry,
    writer::{WriterHandle, WriterOptions},
    WriteJob,
};

#[cfg(unix)]
struct Router {
    writer: WriterHandle,
    notify: Option<mpsc::Sender<Cancellation>>,
}
#[cfg(unix)]
impl Dispatcher for Router {
    fn dispatch(
        &self,
        request: Request,
        options: JobOptions,
    ) -> Result<JobHandle<Vec<u8>>, StoreError> {
        let cancellation = options.cancellation.clone();
        let operation = match (request.kind, request.method.as_str()) {
            (JobKind::Write, "fixture.put") => {
                if request.payload.len() < 8 {
                    return Err(invalid("missing id"));
                }
                WriteJob::new(options, move |tx| {
                    let id = u64::from_le_bytes(request.payload[..8].try_into().unwrap());
                    tx.execute(
                        "INSERT INTO notes VALUES(?,?) ON CONFLICT(id) DO NOTHING",
                        rusqlite::params![id, &request.payload[8..]],
                    )?;
                    Ok(b"committed".to_vec())
                })
            }
            (JobKind::Read, "fixture.get") => {
                if request.payload.len() != 8 {
                    return Err(invalid("invalid id"));
                }
                WriteJob::new(options, move |tx| {
                    let id = u64::from_le_bytes(request.payload[..8].try_into().unwrap());
                    Ok(tx.query_row("SELECT body FROM notes WHERE id=?", [id], |r| r.get(0))?)
                })
            }
            _ => return Err(invalid("unknown method or mode")),
        };
        let job = self.writer.submit(operation)?;
        if let Some(notify) = &self.notify {
            let _ = notify.send(cancellation);
        }
        Ok(job)
    }
}
#[cfg(unix)]
fn endpoint(directory: &Path) -> StoreEndpoint {
    StoreEndpoint::UnixSocket {
        path: directory.join("ipc/store.sock"),
    }
}
#[cfg(unix)]
async fn setup(
    directory: &Path,
    notify: Option<mpsc::Sender<Cancellation>>,
) -> (WriterHandle, StoreEndpoint, DaemonIpc) {
    let writer = Registry
        .open(
            DbId::new(directory.join("database")).unwrap(),
            WriterOptions::default(),
        )
        .unwrap();
    writer.ready().await.unwrap();
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            tx.execute_batch("CREATE TABLE IF NOT EXISTS notes(id INTEGER PRIMARY KEY,body BLOB)")?;
            Ok(())
        }))
        .unwrap()
        .await
        .unwrap();
    let endpoint = endpoint(directory);
    let server = DaemonIpc::start(
        endpoint.clone(),
        Arc::new(Router {
            writer: writer.clone(),
            notify,
        }),
    )
    .unwrap();
    (writer, endpoint, server)
}
fn put(id: u64) -> Request {
    let mut payload = id.to_le_bytes().to_vec();
    payload.extend_from_slice(b"hello");
    Request {
        request_id: 1,
        kind: JobKind::Write,
        method: "fixture.put".into(),
        payload,
    }
}
fn get(id: u64) -> Request {
    Request {
        request_id: 2,
        kind: JobKind::Read,
        method: "fixture.get".into(),
        payload: id.to_le_bytes().to_vec(),
    }
}
#[cfg(unix)]
async fn count(writer: &WriterHandle) -> i64 {
    writer
        .submit(WriteJob::new(JobOptions::default(), |tx| {
            Ok(tx.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))?)
        }))
        .unwrap()
        .await
        .unwrap()
}

#[cfg(unix)]
#[tokio::test]
async fn real_socket_routes_only_registered_modes_and_acknowledges_committed_writes() {
    let d = tempfile::tempdir().unwrap();
    let (writer, endpoint, server) = setup(d.path(), None).await;
    assert_eq!(
        request(endpoint.clone(), put(1), JobOptions::default())
            .unwrap()
            .await
            .unwrap(),
        b"committed"
    );
    assert_eq!(
        request(endpoint.clone(), get(1), JobOptions::default())
            .unwrap()
            .await
            .unwrap(),
        b"hello"
    );
    let mut wrong = put(2);
    wrong.kind = JobKind::Read;
    assert!(matches!(
        request(endpoint.clone(), wrong, JobOptions::default())
            .unwrap()
            .await,
        Err(StoreError::Invalid(_))
    ));
    let mut sql = put(2);
    sql.method = "sql".into();
    assert!(matches!(
        request(endpoint, sql, JobOptions::default()).unwrap().await,
        Err(StoreError::Invalid(_))
    ));
    assert_eq!(count(&writer).await, 1);
    server.shutdown().unwrap().await.unwrap();
    writer.shutdown().await.unwrap();
}

#[cfg(unix)]
#[test]
fn subprocess_client_entrypoint() {
    let Some(path) = std::env::var_os("CTOX_STORE_TEST_IPC_ENDPOINT") else {
        return;
    };
    let endpoint = StoreEndpoint::UnixSocket { path: path.into() };
    assert_eq!(
        request(endpoint.clone(), put(13), JobOptions::default())
            .unwrap()
            .blocking_wait()
            .unwrap(),
        b"committed"
    );
    assert_eq!(
        request(endpoint, get(13), JobOptions::default())
            .unwrap()
            .blocking_wait()
            .unwrap(),
        b"hello"
    );
}
#[cfg(unix)]
#[tokio::test]
async fn separate_cli_process_uses_ipc_without_opening_the_database() {
    let d = tempfile::tempdir().unwrap();
    let (writer, endpoint, server) = setup(d.path(), None).await;
    let StoreEndpoint::UnixSocket { path } = endpoint else {
        unreachable!()
    };
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ipc::tests::subprocess_client_entrypoint",
                "--nocapture",
            ])
            .env("CTOX_STORE_TEST_IPC_ENDPOINT", path)
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(count(&writer).await, 1);
    server.shutdown().unwrap().await.unwrap();
    writer.shutdown().await.unwrap();
}

#[test]
fn oversized_or_untyped_requests_are_rejected_before_connection() {
    let endpoint = StoreEndpoint::UnixSocket {
        path: PathBuf::from("/nonexistent/store.sock"),
    };
    let mut request = put(1);
    request.payload = vec![0; MAX_FRAME + 1];
    assert!(matches!(
        super::request(endpoint.clone(), request, JobOptions::default()),
        Err(StoreError::Invalid(_))
    ));
    let mut request = put(1);
    request.method = "SELECT * FROM secrets".into();
    assert!(matches!(
        super::request(endpoint, request, JobOptions::default()),
        Err(StoreError::Invalid(_))
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn deadline_cancels_a_queued_store_job_before_any_effect() {
    let d = tempfile::tempdir().unwrap();
    let (notify, queued) = mpsc::channel();
    let (writer, endpoint, server) = setup(d.path(), Some(notify)).await;
    let (entered_send, entered) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let blocker = writer
        .submit(WriteJob::new(JobOptions::default(), move |_| {
            entered_send.send(()).unwrap();
            released.recv().unwrap();
            Ok(())
        }))
        .unwrap();
    tokio::task::spawn_blocking(move || entered.recv().unwrap())
        .await
        .unwrap();
    let pending = request(
        endpoint,
        put(1),
        JobOptions {
            deadline: Some(Instant::now() + Duration::from_millis(300)),
            ..Default::default()
        },
    )
    .unwrap();
    let cancellation =
        tokio::task::spawn_blocking(move || queued.recv_timeout(Duration::from_secs(2)).unwrap())
            .await
            .unwrap();
    assert!(matches!(
        pending.await,
        Err(StoreError::DeadlineExceeded) | Err(StoreError::Closed)
    ));
    tokio::task::spawn_blocking(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !cancellation.is_cancelled() && Instant::now() < deadline {
            thread::sleep(TICK);
        }
        assert!(cancellation.is_cancelled());
    })
    .await
    .unwrap();
    release.send(()).unwrap();
    blocker.await.unwrap();
    assert_eq!(count(&writer).await, 0);
    server.shutdown().unwrap().await.unwrap();
    writer.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn daemon_shutdown_cancels_requests_and_releases_lifecycle_ownership() {
    let d = tempfile::tempdir().unwrap();
    let (notify, queued) = mpsc::channel();
    let (writer, endpoint, server) = setup(d.path(), Some(notify)).await;
    let (entered_send, entered) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let blocker = writer
        .submit(WriteJob::new(JobOptions::default(), move |_| {
            entered_send.send(()).unwrap();
            released.recv().unwrap();
            Ok(())
        }))
        .unwrap();
    tokio::task::spawn_blocking(move || entered.recv().unwrap())
        .await
        .unwrap();
    let pending = request(endpoint.clone(), put(1), JobOptions::default()).unwrap();
    let cancellation =
        tokio::task::spawn_blocking(move || queued.recv_timeout(Duration::from_secs(2)).unwrap())
            .await
            .unwrap();
    server.shutdown().unwrap().await.unwrap();
    assert!(cancellation.is_cancelled());
    assert!(pending.await.is_err());
    release.send(()).unwrap();
    blocker.await.unwrap();
    assert_eq!(count(&writer).await, 0);
    writer.shutdown().await.unwrap();
    let id = writer.db_id().clone();
    tokio::task::spawn_blocking(move || drop(MaintenanceOwner::acquire(&endpoint, id).unwrap()))
        .await
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn pool_counts_queued_connections_and_never_dispatches_the_65th() {
    let d = tempfile::tempdir().unwrap();
    let (notify, queued) = mpsc::channel();
    let (writer, endpoint, server) = setup(d.path(), Some(notify)).await;
    let (entered_send, entered) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let blocker = writer
        .submit(WriteJob::new(JobOptions::default(), move |_| {
            entered_send.send(()).unwrap();
            released.recv().unwrap();
            Ok(())
        }))
        .unwrap();
    tokio::task::spawn_blocking(move || entered.recv().unwrap())
        .await
        .unwrap();
    let StoreEndpoint::UnixSocket { path } = endpoint else {
        unreachable!()
    };
    let held = tokio::task::spawn_blocking(move || {
        let mut held = Vec::new();
        for index in 0..POOL_SIZE {
            let mut stream = std::os::unix::net::UnixStream::connect(&path).unwrap();
            stream
                .write_all(&request_bytes(&put(index as u64), MAX_LIFETIME))
                .unwrap();
            held.push(stream);
        }
        let mut tokens = Vec::new();
        for _ in 0..POOL_SIZE {
            tokens.push(queued.recv_timeout(Duration::from_secs(3)).unwrap());
        }
        let mut extra = std::os::unix::net::UnixStream::connect(path).unwrap();
        extra
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let _ = extra.write_all(&request_bytes(&put(100), MAX_LIFETIME));
        let mut byte = [0];
        match extra.read(&mut byte) {
            Ok(0) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
                ) => {}
            other => panic!("65th connection was not rejected: {other:?}"),
        }
        assert!(queued.try_recv().is_err());
        (held, tokens)
    })
    .await
    .unwrap();
    server.shutdown().unwrap().await.unwrap();
    assert!(held.1.iter().all(Cancellation::is_cancelled));
    drop(held);
    release.send(()).unwrap();
    blocker.await.unwrap();
    assert_eq!(count(&writer).await, 0);
    writer.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn offline_reads_require_both_stopped_daemon_and_exclusive_database_ownership() {
    let d = tempfile::tempdir().unwrap();
    let (writer, endpoint, server) = setup(d.path(), None).await;
    let id = writer.db_id().clone();
    assert!(matches!(
        MaintenanceOwner::acquire(&endpoint, id.clone()),
        Err(StoreError::AlreadyOwned)
    ));
    server.shutdown().unwrap().await.unwrap();
    assert!(matches!(
        MaintenanceOwner::acquire(&endpoint, id.clone()),
        Err(StoreError::AlreadyOwned)
    ));
    writer.shutdown().await.unwrap();
    tokio::task::spawn_blocking(move || {
        let maintenance = MaintenanceOwner::acquire(&endpoint, id).unwrap();
        let read = maintenance.open_read_only().unwrap();
        assert_eq!(
            read.read(|connection| Ok(connection.query_row(
                "SELECT COUNT(*) FROM notes",
                [],
                |r| r.get::<_, i64>(0)
            )?))
            .unwrap(),
            0
        );
        assert!(read
            .read(|connection| {
                connection.execute("INSERT INTO notes VALUES(1,'forbidden')", [])?;
                Ok(())
            })
            .is_err());
        assert!(matches!(
            DaemonIpc::start(
                endpoint.clone(),
                Arc::new(Router {
                    writer: writer.clone(),
                    notify: None
                })
            ),
            Err(StoreError::AlreadyOwned)
        ));
        drop(read);
        drop(maintenance);
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[test]
fn os_peer_authentication_rejects_a_different_expected_user() {
    let (first, _second) = std::os::unix::net::UnixStream::pair().unwrap();
    let stream = unix::Stream(first);
    stream.authenticate(true).unwrap();
    assert_eq!(
        stream
            .authenticate_uid(unix::uid().wrapping_add(1))
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
}

#[cfg(unix)]
#[tokio::test]
async fn directory_socket_and_cleanup_refuse_foreign_paths() {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    let (writer, endpoint, server) = setup(d.path(), None).await;
    assert!(matches!(
        DaemonIpc::start(
            endpoint.clone(),
            Arc::new(Router {
                writer: writer.clone(),
                notify: None
            })
        ),
        Err(StoreError::AlreadyOwned)
    ));
    let StoreEndpoint::UnixSocket { path } = &endpoint else {
        unreachable!()
    };
    fs::remove_file(path).unwrap();
    fs::write(path, b"replacement").unwrap();
    server.shutdown().unwrap().await.unwrap();
    assert_eq!(fs::read(path).unwrap(), b"replacement");
    assert!(DaemonIpc::start(
        endpoint.clone(),
        Arc::new(Router {
            writer: writer.clone(),
            notify: None
        })
    )
    .is_err());
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(DaemonIpc::start(
        endpoint,
        Arc::new(Router {
            writer: writer.clone(),
            notify: None
        })
    )
    .is_err());
    writer.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn malformed_and_partial_frames_do_not_dispatch_or_allocate_unbounded_payloads() {
    let d = tempfile::tempdir().unwrap();
    let (notify, queued) = mpsc::channel();
    let (writer, endpoint, server) = setup(d.path(), Some(notify)).await;
    let StoreEndpoint::UnixSocket { path } = endpoint else {
        unreachable!()
    };
    tokio::task::spawn_blocking(move || {
        for oversize in [true, false] {
            let mut stream = std::os::unix::net::UnixStream::connect(&path).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = request_bytes(&put(1), Duration::from_millis(20));
            if oversize {
                bytes[24..28].copy_from_slice(&((MAX_FRAME + 1) as u32).to_le_bytes());
            }
            stream.write_all(&bytes[..28]).unwrap();
            let mut byte = [0];
            match stream.read(&mut byte) {
                Ok(0) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe
                    ) => {}
                other => panic!("invalid/incomplete frame did not close before timeout: {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    assert!(queued.try_recv().is_err());
    server.shutdown().unwrap().await.unwrap();
    writer.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn client_rejects_mismatched_correlation_and_oversized_responses() {
    let d = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let path = d.path().join("fake.sock");
    for oversize in [false, true] {
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = StoreEndpoint::UnixSocket { path: path.clone() };
        let fake = tokio::task::spawn_blocking(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(accepted) => break accepted,
                    Err(error)
                        if error.kind() == io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(TICK)
                    }
                    other => panic!("fixture accept did not complete: {other:?}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut header = [0; 28];
            stream.read_exact(&mut header).unwrap();
            let extra = u32::from_le_bytes(header[20..24].try_into().unwrap()) as usize
                + u32::from_le_bytes(header[24..28].try_into().unwrap()) as usize;
            let mut body = vec![0; extra];
            stream.read_exact(&mut body).unwrap();
            let mut response = Vec::from(&b"CSR4"[..]);
            response.extend_from_slice(&(if oversize { 1u64 } else { 999 }).to_le_bytes());
            response.extend_from_slice(&[0; 4]);
            response.extend_from_slice(
                &(if oversize { (MAX_FRAME + 1) as u32 } else { 0 }).to_le_bytes(),
            );
            let _ = stream.write_all(&response);
        });
        assert!(matches!(
            request(endpoint, put(1), JobOptions::default())
                .unwrap()
                .await,
            Err(StoreError::Invalid(_))
        ));
        fake.await.unwrap();
        fs::remove_file(&path).unwrap();
    }
}

#[cfg(target_os = "linux")]
#[test]
fn full_unix_listen_backlog_cannot_outlive_the_connect_deadline() {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn listen(fd: i32, backlog: i32) -> i32;
    }
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("backlog.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    assert_eq!(unsafe { listen(listener.as_raw_fd(), 1) }, 0);
    let (send, receive) = mpsc::channel();
    let worker = thread::spawn(move || {
        let mut sockets = Vec::new();
        let mut timed_out = false;
        for _ in 0..8 {
            match unix::connect_path(
                &path,
                Instant::now() + Duration::from_millis(50),
                &AtomicBool::new(false),
                &Cancellation::default(),
            ) {
                Ok(socket) => sockets.push(socket),
                Err(error) => {
                    timed_out = error.kind() == io::ErrorKind::TimedOut;
                    break;
                }
            }
        }
        let _ = send.send((sockets.len(), timed_out));
    });
    // A regression to blocking connect fails instead of hanging the test suite:
    // close this listener before joining a stalled fixture worker.
    let result = receive.recv_timeout(Duration::from_secs(2));
    drop(listener);
    worker.join().unwrap();
    let (connected, timed_out) =
        result.expect("connect exceeded its deadline behind a full backlog");
    assert!(connected > 0);
    assert!(timed_out, "fixture did not reach a bounded full backlog");
}
