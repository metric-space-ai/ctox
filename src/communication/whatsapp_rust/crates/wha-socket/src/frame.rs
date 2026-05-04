//! Async, length-prefixed framing on top of an arbitrary websocket sink/stream.
//!
//! [`FrameSocket`] is the Rust equivalent of `whatsmeow/socket/framesocket.go`:
//! it owns the websocket, prepends the WA-conn header on the very first frame,
//! and exposes both a `send_frame` method and an mpsc receiver of inbound
//! frames so the higher layers can drive their own state machines.

use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use futures::{stream::SplitSink, SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::{connect_async_with_config, MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::constants::ORIGIN;

use crate::constants::{FRAME_LENGTH_SIZE, FRAME_MAX_SIZE, WA_CONN_HEADER};
use crate::error::SocketError;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type Sink = SplitSink<WsStream, Message>;

/// Simple sender owned by the noise/handshake layer. The `Arc<Mutex<…>>` lets
/// the noise socket and the handshake share the same write half.
pub type SharedSink = Arc<Mutex<Sink>>;

pub struct FrameSocket {
    sink: SharedSink,
    /// Oneshot inbox for inbound frames already split out of the websocket
    /// message stream and reassembled to logical frames.
    pub frames: mpsc::UnboundedReceiver<Bytes>,
    pub frames_tx: mpsc::UnboundedSender<Bytes>,
    header_pending: Option<[u8; 4]>,
    is_connected: bool,
}

impl FrameSocket {
    /// Connect to `url` and spawn the read pump. The header is sent on the
    /// first call to `send_frame`, mirroring the upstream behaviour where the
    /// bytes go out atomically with the first noise message.
    pub async fn connect(url: &str) -> Result<Self, SocketError> {
        // Build a client request explicitly so we can set Origin —
        // WA's edge enforces this and silently drops connections without it.
        let mut req = url
            .into_client_request()
            .map_err(|e| SocketError::DialFailed(format!("bad url: {e}")))?;
        let host = req
            .uri()
            .host()
            .ok_or_else(|| SocketError::DialFailed("url has no host".into()))?
            .to_owned();
        let headers = req.headers_mut();
        headers.insert("Origin", ORIGIN.parse().unwrap());
        headers.insert("Host", host.parse().unwrap());
        headers.insert("Sec-WebSocket-Key", generate_key().parse().unwrap());
        headers.insert("Sec-WebSocket-Version", "13".parse().unwrap());
        headers.insert("Connection", "Upgrade".parse().unwrap());
        headers.insert("Upgrade", "websocket".parse().unwrap());

        let (ws, _resp) = connect_async_with_config(req, Some(WebSocketConfig::default()), false)
            .await
            .map_err(|e| SocketError::DialFailed(e.to_string()))?;
        let (sink, mut stream) = ws.split();
        let sink = Arc::new(Mutex::new(sink));
        let (frames_tx, frames_rx) = mpsc::unbounded_channel();
        let read_tx = frames_tx.clone();

        tokio::spawn(async move {
            // Reassemble length-prefixed frames out of binary websocket messages.
            let mut buffer = BytesMut::new();
            while let Some(msg) = stream.next().await {
                let bytes = match msg {
                    Ok(Message::Binary(b)) => b,
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => continue,
                };
                buffer.extend_from_slice(&bytes);
                while let Some(frame) = take_frame(&mut buffer) {
                    if read_tx.send(frame).is_err() {
                        return;
                    }
                }
            }
        });

        Ok(Self {
            sink,
            frames: frames_rx,
            frames_tx,
            header_pending: Some(WA_CONN_HEADER),
            is_connected: true,
        })
    }

    /// Returns true while the underlying websocket is alive.
    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    /// Send one logical frame. The first frame on a new socket is silently
    /// prefixed with the WA-conn header.
    pub async fn send_frame(&mut self, payload: &[u8]) -> Result<(), SocketError> {
        if payload.len() >= FRAME_MAX_SIZE {
            return Err(SocketError::FrameTooLarge(payload.len()));
        }
        let header_len = self.header_pending.map(|_| 4).unwrap_or(0);
        let total = header_len + FRAME_LENGTH_SIZE + payload.len();
        let mut whole = Vec::with_capacity(total);
        if let Some(h) = self.header_pending.take() {
            whole.extend_from_slice(&h);
        }
        let len = payload.len();
        whole.push((len >> 16) as u8);
        whole.push((len >> 8) as u8);
        whole.push(len as u8);
        whole.extend_from_slice(payload);
        let mut sink = self.sink.lock().await;
        sink.send(Message::Binary(whole.into())).await.map_err(|e| SocketError::Ws(e.to_string()))?;
        Ok(())
    }

    /// Hand out the shared sink so the noise socket can keep using it after
    /// the handshake hands the framesocket off.
    pub fn shared_sink(&self) -> SharedSink {
        self.sink.clone()
    }

    /// Close the underlying websocket gracefully.
    pub async fn close(&mut self) -> Result<(), SocketError> {
        let mut sink = self.sink.lock().await;
        let _ = sink.close().await;
        self.is_connected = false;
        Ok(())
    }
}

fn take_frame(buf: &mut BytesMut) -> Option<Bytes> {
    if buf.len() < FRAME_LENGTH_SIZE {
        return None;
    }
    let len = ((buf[0] as usize) << 16) | ((buf[1] as usize) << 8) | (buf[2] as usize);
    if buf.len() < FRAME_LENGTH_SIZE + len {
        return None;
    }
    let _ = buf.split_to(FRAME_LENGTH_SIZE);
    Some(buf.split_to(len).freeze())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_frame_handles_partial_buffer() {
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&[0, 0, 5, b'h', b'e']);
        assert!(take_frame(&mut buf).is_none(), "should wait for tail");
        buf.extend_from_slice(b"llo");
        let f = take_frame(&mut buf).unwrap();
        assert_eq!(&f[..], b"hello");
        assert!(buf.is_empty());
    }

    #[test]
    fn take_frame_handles_concatenated_frames() {
        let mut buf = BytesMut::new();
        // two frames: "ab" and "cd"
        buf.extend_from_slice(&[0, 0, 2, b'a', b'b', 0, 0, 2, b'c', b'd']);
        assert_eq!(&take_frame(&mut buf).unwrap()[..], b"ab");
        assert_eq!(&take_frame(&mut buf).unwrap()[..], b"cd");
        assert!(take_frame(&mut buf).is_none());
    }
}
