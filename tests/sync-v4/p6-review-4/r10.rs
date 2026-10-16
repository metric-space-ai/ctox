use super::*;
use tokio::sync::Notify;

// The failing native engine has a REAL child effect. The fixture retains its
// observation/cleanup handle so even the defective owner's drop cannot leak it.
struct Effect(Mutex<Option<std::process::Child>>);
impl Effect {
    fn alive(&self) -> bool {
        self.0.lock().unwrap().as_mut().is_some_and(|child| child.try_wait().unwrap().is_none())
    }
    fn stop(&self) {
        if let Some(mut child) = self.0.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Drop for Effect { fn drop(&mut self) { self.stop(); } }
struct DrainFactory { effect: Arc<Effect>, failed: Arc<Notify> }
struct Undrained(Arc<Effect>, Arc<Notify>);
impl RunningEngine for Undrained {
    fn shutdown(&mut self) -> EngineFuture<()> {
        let effect = self.0.clone();
        let failed = self.1.clone();
        Box::pin(async move {
            if effect.alive() {
                failed.notify_one();
                return Err(StoreError::Busy);
            }
            Ok(())
        })
    }
}
impl EngineFactory for DrainFactory {
    fn name(&self) -> &'static str { "p6-r10-undrained" }
    fn start(&self, _: Arc<StoreInputs>) -> EngineFuture<Box<dyn RunningEngine>> {
        let engine = Undrained(self.effect.clone(), self.failed.clone());
        Box::pin(async move {
            *engine.0.0.lock().unwrap() = Some(std::process::Command::new("sleep").arg("30").spawn()?);
            Ok(Box::new(engine) as Box<dyn RunningEngine>)
        })
    }
}
struct FailedStart;
impl EngineFactory for FailedStart {
    fn name(&self) -> &'static str { "p6-r10-failed-start" }
    fn start(&self, _: Arc<StoreInputs>) -> EngineFuture<Box<dyn RunningEngine>> {
        Box::pin(async { Err(StoreError::Cancelled) })
    }
}

// Actual Foreground::start error unwind, real Store files and IPC custody.
// Run alone: native factory registration is global. All waits are bounded.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r10_failed_start_must_retain_custody_until_engine_drains() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().to_owned();
    let effect = Arc::new(Effect(Mutex::new(None)));
    let failed = Arc::new(Notify::new());
    bind_engine(Arc::new(DrainFactory { effect: effect.clone(), failed: failed.clone() })).unwrap();
    bind_engine(Arc::new(FailedStart)).unwrap();
    let boot_root = root.clone();
    let mut boot = tokio::spawn(async move { Foreground::start(boot_root, Handle::current()).await });
    tokio::time::timeout(std::time::Duration::from_secs(10), failed.notified()).await.unwrap();
    // A retained owner/retry remains pending; today's caller unwinds.
    let returned = tokio::time::timeout(std::time::Duration::from_millis(200), &mut boot).await;
    let effect_alive = effect.alive();
    let id = DbId::new(crate::paths::sync_db(&root)).unwrap();
    let custody = ctox_store::ipc::MaintenanceOwner::acquire(&queue_store::endpoint(&root), id);
    let retained = custody.is_err();
    drop(custody);
    effect.stop();
    if returned.is_err() {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), &mut boot).await;
        if !boot.is_finished() { boot.abort(); let _ = boot.await; }
    }
    assert!(effect_alive, "drain-failure fixture had no live native child effect");
    assert!(retained, "R10: maintenance acquired custody while a native engine was still undrained");
}
