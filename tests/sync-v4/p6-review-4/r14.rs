// Included beside the production OwnerAdapter; no public test API is added.
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r14_attach_syscall_probe() {
    use std::future::Future;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("p6-r14-owned.sqlite3");
    let fixture = ctox_store::testkit::StoreFixture::open(ctox_store::DbId::new(&path).unwrap()).await.unwrap();
    eprintln!("R14_PATH {}", path.display());
    let mut attach = Box::pin(super::OwnerAdapter::attach(&path, fixture.writer(), fixture.readers()));
    // Delimit EVERY poll with the actual kernel TID. Tokio may migrate a task;
    // filesystem probes on reader/store/blocking threads remain permitted.
    let adapter = std::future::poll_fn(|cx| {
        let tid = unsafe { libc::syscall(libc::SYS_gettid) };
        eprintln!("R14_POLL_BEGIN tid={tid}");
        let result = attach.as_mut().poll(cx);
        eprintln!("R14_POLL_END tid={tid}");
        result
    }).await.unwrap();
    drop(adapter);
    fixture.shutdown().await.unwrap();
}
