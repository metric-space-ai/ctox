//! The process runtime. Subsystems borrow its handle; they never own/drop it.
use std::sync::{Mutex, OnceLock};
use tokio::runtime::{Builder, Runtime};

pub(crate) fn shared() -> std::io::Result<&'static Runtime> {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    static INITIALIZING: Mutex<()> = Mutex::new(());
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let _guard = INITIALIZING.lock().unwrap_or_else(|p| p.into_inner());
    if RUNTIME.get().is_none() {
        let workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(8);
        let runtime = Builder::new_multi_thread()
            .worker_threads(workers)
            .max_blocking_threads(16)
            .thread_name("ctox-runtime")
            .enable_all()
            .build()?;
        // Only the initializing thread can set this cell.
        let _ = RUNTIME.set(runtime);
    }
    Ok(RUNTIME.get().expect("initialized process runtime"))
}
