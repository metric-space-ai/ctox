use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};
use url::Url;

pub type SignalSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn connect_to_signal_server(
    urls: &[String],
    auth_token: &str,
    client_id: &str,
) -> Result<SignalSocket> {
    let mut last_error = None;

    for raw_url in urls {
        match build_signal_url(raw_url, auth_token, client_id) {
            Ok(url) => match connect_async(url.as_str()).await {
                Ok((socket, _)) => return Ok(socket),
                Err(error) => last_error = Some(anyhow!("{}: {error}", raw_url)),
            },
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("no signaling URL configured")))
}

pub fn build_signal_url(raw_url: &str, auth_token: &str, client_id: &str) -> Result<Url> {
    let mut url =
        Url::parse(raw_url).with_context(|| format!("failed to parse signaling URL {raw_url}"))?;
    if url.scheme() != "wss" {
        anyhow::bail!("signaling URL must use wss://: {raw_url}");
    }
    if !auth_token.trim().is_empty() {
        url.query_pairs_mut()
            .append_pair("token", auth_token.trim());
    }
    if !client_id.trim().is_empty() {
        url.query_pairs_mut()
            .append_pair("client", client_id.trim());
    }
    Ok(url)
}

pub async fn send_json<T: serde::Serialize>(writer: &mut SignalSocket, value: &T) -> Result<()> {
    let payload = serde_json::to_string(value)?;
    writer.send(Message::Text(payload)).await?;
    Ok(())
}

pub async fn next_json<T: serde::de::DeserializeOwned>(
    reader: &mut SignalSocket,
) -> Result<Option<T>> {
    while let Some(message) = reader.next().await {
        let message = message?;
        match message {
            Message::Text(text) => return Ok(Some(serde_json::from_str(&text)?)),
            Message::Binary(bytes) => return Ok(Some(serde_json::from_slice(&bytes)?)),
            Message::Ping(payload) => {
                reader.send(Message::Pong(payload)).await?;
            }
            Message::Pong(_) => {}
            Message::Close(_) => return Ok(None),
            Message::Frame(_) => {}
        }
    }
    Ok(None)
}
