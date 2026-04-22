use std::io;
use std::pin::Pin;
use std::time::Duration;

use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::io::split;

pub type LocalResponsesReadHalf = Pin<Box<dyn AsyncRead + Send>>;
pub type LocalResponsesWriteHalf = Pin<Box<dyn AsyncWrite + Send>>;

pub async fn connect_local_responses_transport(
    endpoint: &str,
    timeout: Duration,
) -> io::Result<(LocalResponsesReadHalf, LocalResponsesWriteHalf)> {
    #[cfg(unix)]
    {
        use tokio::net::UnixStream;

        let stream = tokio::time::timeout(timeout, UnixStream::connect(endpoint))
            .await
            .map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, "local transport connect timed out")
            })??;
        let (reader, writer) = split(stream);
        return Ok((Box::pin(reader), Box::pin(writer)));
    }

    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;
        use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;

        let deadline = tokio::time::Instant::now() + timeout;
        let stream = loop {
            match ClientOptions::new().open(endpoint) {
                Ok(stream) => break stream,
                Err(err)
                    if err.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)
                        && tokio::time::Instant::now() < deadline =>
                {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(err) => return Err(err),
            }
        };
        let (reader, writer) = split(stream);
        return Ok((Box::pin(reader), Box::pin(writer)));
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (endpoint, timeout);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "local Responses transport is unsupported on this platform",
        ))
    }
}
