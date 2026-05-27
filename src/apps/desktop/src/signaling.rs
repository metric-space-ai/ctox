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
    // CTOX-RXDB-Discovery-Params: müssen mit `src/apps/business-os/shared/sync.js`
    // konsistent sein. Browser sendet (siehe signalingUrlWithBrowserMetadata,
    // Zeile 1392-1418) `role=browser` + `protocol=ctox-rxdb-protocol-v1` +
    // `instance_id=<sync_room>` + `cap=...`. Ohne `role=ctox_instance`
    // klassifiziert der Hub diesen Peer als role="unknown", schließt die
    // Signaling-Connection sofort (siehe Reconnect-Loop in
    // /tmp/ctox-desktop-host.log mit 19.5k closed sessions) und die
    // Browser-RxDB-Replikation startet nie — Symptom: alle
    // `WebRTC replication failed for ...`-Errors plus permanent rotes
    // "CTOX ARBEITET NICHT" im Header. Der Browser-seitige
    // `hasNativePeerProtocolEvidence` (shared/sync.js:895-913) verlangt
    // zusätzlich beide ctox-peer-session-v1 und ctox-checkpoint-epoch-v1
    // Capabilities im Peer-Handshake-Set.
    {
        let mut query = url.query_pairs_mut();
        if !auth_token.trim().is_empty() {
            query.append_pair("token", auth_token.trim());
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            // 24-Stunden-Fenster wie der Browser-Pfad
            // (business-os/shared/sync.js:1407-1410).
            query.append_pair("token_iat", &now.to_string());
            query.append_pair("token_exp", &(now + 24 * 60 * 60).to_string());
        }
        if !client_id.trim().is_empty() {
            query.append_pair("client", client_id.trim());
        }
        query.append_pair("role", "ctox_instance");
        query.append_pair("protocol", "ctox-rxdb-protocol-v1");
        if !client_id.trim().is_empty() {
            query.append_pair("instance_id", client_id.trim());
        }
        query.append_pair("cap", "ctox-control-plane-v1");
        query.append_pair("cap", "ctox-rxdb-browser-v1");
        query.append_pair("cap", "ctox-file-chunks-v1");
        query.append_pair("cap", "ctox-schema-hash-v1");
        query.append_pair("cap", "ctox-peer-session-v1");
        query.append_pair("cap", "ctox-checkpoint-epoch-v1");
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
