//! Install the daemon subscriber and forward webrtc-rs `log` records into it.
//! Called once from the binary entry point, never from a reusable library constructor.
pub fn install_daemon_log_bridge() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    tracing_log::LogTracer::init()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    #[derive(Clone)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);
    impl Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn p4_log_warning_reaches_tracing_once() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let buffer = Buffer(bytes.clone());
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || buffer.clone())
            .finish();
        // Install only the facade bridge here; preserve the test's scoped subscriber.
        tracing_log::LogTracer::init().unwrap();
        tracing::subscriber::with_default(subscriber, || {
            log::warn!(target: "webrtc::peer_connection", "p4-bridge-warning");
        });
        let output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert_eq!(output.matches("p4-bridge-warning").count(), 1, "{output}");
        assert!(output.contains("WARN"));
        assert!(output.contains("webrtc::peer_connection"));
    }
}
