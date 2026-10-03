use super::*;

#[tokio::test]
async fn non_authority_service_uses_owned_socket_and_shutdown_drains_its_connections() {
    use crate::ipc::LocalIpcStream;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    struct Probe(Arc<AtomicUsize>);
    struct Active(Arc<AtomicUsize>);
    impl Drop for Active {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    impl IpcService for Probe {
        fn serve_connection(
            &self,
            mut stream: Box<dyn LocalIpcStream>,
        ) -> crate::ipc::IpcServiceFuture<'_> {
            Box::pin(async move {
                self.0.fetch_add(1, Ordering::SeqCst);
                let _active = Active(self.0.clone());
                stream.write_all(b"probe").await?;
                let mut byte = [0];
                stream.read_exact(&mut byte).await?;
                Ok(())
            })
        }
    }
    let directory = private_ipc_directory().unwrap();
    let active = Arc::new(AtomicUsize::new(0));
    let host = LocalIpcHost::start(directory.path().into(), Arc::new(Probe(active.clone())))
        .await
        .unwrap();
    let endpoint = host.endpoint().to_path_buf();
    let mut clients = Vec::new();
    for _ in 0..2 {
        let mut client = tokio::net::UnixStream::connect(&endpoint).await.unwrap();
        let mut reply = [0; 5];
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.read_exact(&mut reply),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&reply, b"probe");
        clients.push(client);
    }
    assert_eq!(active.load(Ordering::SeqCst), 2);
    tokio::time::timeout(std::time::Duration::from_secs(2), host.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        active.load(Ordering::SeqCst),
        0,
        "all service futures dropped before shutdown returns"
    );
    assert!(!endpoint.exists());
    for mut client in clients {
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), client.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn failed_listener_join_can_be_followed_by_shutdown() {
    let task = tokio::spawn(std::future::pending::<io::Result<()>>());
    task.abort();
    let mut host = LocalIpcHost {
        endpoint: PathBuf::new(),
        stop: None,
        task: Some(task),
    };
    assert!(host.wait_stopped().await.is_err());
    // A completed failed JoinHandle must be consumed, not polled a second time.
    host.shutdown().await.unwrap();
}

#[test]
fn private_host_recovers_only_dead_sockets_and_never_replaces_a_live_listener() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let path = root.path().join("authority.sock");
    let live = StdListener::bind(&path).unwrap();
    assert_eq!(
        BoundSocket::bind(root.path()).err().unwrap().kind(),
        io::ErrorKind::AddrInUse
    );
    assert!(path.exists());
    drop(live);
    let host = BoundSocket::bind(root.path()).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        BoundSocket::bind(root.path()).err().unwrap().kind(),
        io::ErrorKind::AddrInUse
    );
    drop(host);
    assert!(!path.exists());
    assert!(BoundSocket::bind(root.path()).is_ok());
}

#[test]
fn host_preserves_foreign_files_and_rejects_shared_or_symlinked_directories() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let path = root.path().join("authority.sock");
    fs::write(&path, b"unrelated file").unwrap();
    assert!(BoundSocket::bind(root.path()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"unrelated file");
    fs::remove_file(&path).unwrap();
    let target = root.path().join("target");
    fs::write(&target, b"keep me").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert!(BoundSocket::bind(root.path()).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"keep me");
    fs::remove_file(&path).unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(BoundSocket::bind(root.path()).is_err());
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(root.path(), &alias).unwrap();
    assert!(BoundSocket::bind(&alias).is_err());
}

#[test]
fn late_host_cleanup_cannot_remove_a_replacement_path() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let host = BoundSocket::bind(root.path()).unwrap();
    let path = host.path.clone();
    fs::remove_file(&path).unwrap();
    fs::write(&path, b"replacement").unwrap();
    drop(host);
    assert_eq!(fs::read(&path).unwrap(), b"replacement");
}
