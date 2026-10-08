// Origin: CTOX
// License: AGPL-3.0-only
//! Runtime retained with the native child, never a publication/ownership grant.
use super::*;
pub(super) struct MachineIo(Option<tokio::runtime::Runtime>);
impl MachineIo {
    pub(super) fn new() -> Result<Self> {
        Ok(Self(Some(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?,
        )))
    }
    pub(super) fn run<T: Send>(
        &self,
        operation: impl FnOnce(&tokio::runtime::Runtime) -> Result<T> + Send,
    ) -> Result<T> {
        let runtime = self.0.as_ref().context("machine runtime retired")?;
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let _entered = runtime.enter();
                    operation(runtime)
                })
                .join()
                .map_err(|_| anyhow::anyhow!("native machine operation panicked"))?
        })
    }
}
impl Drop for MachineIo {
    fn drop(&mut self) {
        if let Some(runtime) = self.0.take() {
            runtime.shutdown_background();
        }
    }
}
pub(super) fn run<T: Send>(
    io: Option<&MachineIo>,
    future: impl std::future::Future<Output = Result<T>> + Send,
) -> Result<T> {
    match io {
        Some(io) => io.run(|runtime| runtime.block_on(future)),
        None => super::super::guest_commands::block_on_guest(future),
    }
}
