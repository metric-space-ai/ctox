#![forbid(unsafe_code)]

use crate::options::OptionSet;
use crate::session::Session;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, FromRequestParts, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tower_http::cors::{Any, CorsLayer};

struct TlsListener {
    tcp: TcpListener,
    acceptor: TlsAcceptor,
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, addr) = match self.tcp.accept().await {
                Ok(t) => t,
                Err(_) => continue,
            };
            match self.acceptor.accept(stream).await {
                Ok(tls) => return (tls, addr),
                Err(_) => continue,
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}

#[derive(Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Serialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

const METHODS: &[&str] = &[
    "aria2.addUri",
    "aria2.addTorrent",
    "aria2.addMetalink",
    "aria2.remove",
    "aria2.forceRemove",
    "aria2.pause",
    "aria2.forcePause",
    "aria2.pauseAll",
    "aria2.forcePauseAll",
    "aria2.unpause",
    "aria2.unpauseAll",
    "aria2.tellStatus",
    "aria2.tellActive",
    "aria2.tellWaiting",
    "aria2.tellStopped",
    "aria2.getUris",
    "aria2.getFiles",
    "aria2.getPeers",
    "aria2.getServers",
    "aria2.changeUri",
    "aria2.changePosition",
    "aria2.getOption",
    "aria2.changeOption",
    "aria2.getGlobalOption",
    "aria2.changeGlobalOption",
    "aria2.purgeDownloadResult",
    "aria2.removeDownloadResult",
    "aria2.getVersion",
    "aria2.getSessionInfo",
    "aria2.getGlobalStat",
    "aria2.saveSession",
    "aria2.shutdown",
    "aria2.forceShutdown",
    "system.listMethods",
    "system.listNotifications",
    "system.multicall",
];

pub async fn serve(session: Arc<Session>, listen_all: bool, port: u16) -> crate::Result<()> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    let g = session.get_global_option().await;
    let secure = matches!(
        g.get("rpc-secure").and_then(|v| v.as_str()),
        Some("true" | "1" | "yes")
    );
    let tls = if secure {
        Some(rpc_tls_acceptor(&g)?)
    } else {
        None
    };
    let app = Router::new()
        .route("/jsonrpc", post(jsonrpc).get(jsonrpc_get))
        .route("/rpc", post(xmlrpc))
        .with_state(session)
        .layer(cors)
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024));
    let addr: SocketAddr = if listen_all {
        ([0, 0, 0, 0], port).into()
    } else {
        ([127, 0, 0, 1], port).into()
    };
    tracing::info!("JSON-RPC listening on {addr} tls={secure}");
    let listener = TcpListener::bind(addr).await?;
    if let Some(acceptor) = tls {
        axum::serve(TlsListener { tcp: listener, acceptor }, app).await?;
    } else {
        axum::serve(listener, app).await?;
    }
    Ok(())
}

fn rpc_tls_acceptor(g: &Value) -> crate::Result<TlsAcceptor> {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    let cert_path = g
        .get("rpc-certificate")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| crate::Error::Rpc("rpc-secure requires --rpc-certificate".into()))?;
    let key_path = g
        .get("rpc-private-key")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| crate::Error::Rpc("rpc-secure requires --rpc-private-key".into()))?;
    let cert_pem = std::fs::read(cert_path)
        .map_err(|e| crate::Error::Rpc(format!("rpc-certificate: {e}")))?;
    let key_pem =
        std::fs::read(key_path).map_err(|e| crate::Error::Rpc(format!("rpc-private-key: {e}")))?;
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&cert_pem)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| crate::Error::Rpc(format!("rpc-certificate PEM: {e}")))?;
    if certs.is_empty() {
        return Err(crate::Error::Rpc("rpc-certificate has no certs".into()));
    }
    let key = PrivateKeyDer::from_pem_slice(&key_pem)
        .map_err(|e| crate::Error::Rpc(format!("rpc-private-key PEM: {e}")))?;
    let provider = rustls::crypto::ring::default_provider();
    let mut cfg = rustls::ServerConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .map_err(|e| crate::Error::Rpc(format!("rpc tls: {e}")))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| crate::Error::Rpc(format!("rpc tls cert/key: {e}")))?;
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(TlsAcceptor::from(Arc::new(cfg)))
}

async fn jsonrpc_get(
    State(session): State<Arc<Session>>,
    req: Request,
) -> impl IntoResponse {
    let upgrade = req
        .headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.eq_ignore_ascii_case("websocket"));
    if !upgrade {
        return (StatusCode::OK, "aria2 JSON-RPC").into_response();
    }
    let headers = req.headers().clone();
    if let Err(resp) = check_basic(&session, &headers).await {
        return resp;
    }
    let (mut parts, _body) = req.into_parts();
    match WebSocketUpgrade::from_request_parts(&mut parts, &session).await {
        Ok(ws) => ws
            .on_upgrade(move |socket| handle_ws(socket, session))
            .into_response(),
        Err(e) => e.into_response(),
    }
}

async fn handle_ws(mut socket: WebSocket, session: Arc<Session>) {
    while let Some(Ok(msg)) = socket.recv().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Binary(b) => String::from_utf8_lossy(&b).into_owned(),
            Message::Ping(p) => {
                let _ = socket.send(Message::Pong(p)).await;
                continue;
            }
            Message::Close(_) => break,
            Message::Pong(_) => continue,
        };
        let g = session.get_global_option().await;
        let max = g
            .get("rpc-max-request-size")
            .and_then(|v| v.as_str())
            .and_then(crate::storage::parse_size)
            .unwrap_or(2 * 1024 * 1024);
        if text.len() as u64 > max {
            let _ = socket
                .send(Message::Text(
                    serde_json::to_string(&RpcResponse {
                        jsonrpc: "2.0".into(),
                        id: None,
                        result: None,
                        error: Some(json!({"code": 1, "message": "request too large"})),
                    })
                    .unwrap_or_else(|_| r#"{"jsonrpc":"2.0","error":{"code":1,"message":"too large"}}"#.into())
                    .into(),
                ))
                .await;
            continue;
        }
        let req: RpcRequest = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                let _ = socket
                    .send(Message::Text(
                        serde_json::to_string(&RpcResponse {
                            jsonrpc: "2.0".into(),
                            id: None,
                            result: None,
                            error: Some(json!({"code": 1, "message": e.to_string()})),
                        })
                        .unwrap_or_default()
                        .into(),
                    ))
                    .await;
                continue;
            }
        };
        let id = req.id.clone();
        let resp = match dispatch(&session, &req).await {
            Ok(result) => RpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: Some(result),
                error: None,
            },
            Err(e) => RpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(json!({"code": 1, "message": e.to_string()})),
            },
        };
        let body = serde_json::to_string(&resp).unwrap_or_else(|_| {
            r#"{"jsonrpc":"2.0","error":{"code":1,"message":"encode"}}"#.into()
        });
        if socket.send(Message::Text(body.into())).await.is_err() {
            break;
        }
    }
}

async fn jsonrpc(
    State(session): State<Arc<Session>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let g = session.get_global_option().await;
    let max = g
        .get("rpc-max-request-size")
        .and_then(|v| v.as_str())
        .and_then(crate::storage::parse_size)
        .unwrap_or(2 * 1024 * 1024);
    if let Some(cl) = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
    {
        if cl > max {
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        }
    }
    if body.len() as u64 > max {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    if let Err(resp) = check_basic(&session, &headers).await {
        return resp;
    }
    let req: RpcRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return Json(RpcResponse {
                jsonrpc: "2.0".into(),
                id: None,
                result: None,
                error: Some(json!({"code": 1, "message": e.to_string()})),
            })
            .into_response();
        }
    };
    let id = req.id.clone();
    match dispatch(&session, &req).await {
        Ok(result) => Json(RpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        })
        .into_response(),
        Err(e) => Json(RpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(json!({"code": 1, "message": e.to_string()})),
        })
        .into_response(),
    }
}

async fn xmlrpc(
    State(session): State<Arc<Session>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let g = session.get_global_option().await;
    let max = g
        .get("rpc-max-request-size")
        .and_then(|v| v.as_str())
        .and_then(crate::storage::parse_size)
        .unwrap_or(2 * 1024 * 1024);
    if body.len() as u64 > max {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    if let Err(resp) = check_basic(&session, &headers).await {
        return resp;
    }
    let xml = String::from_utf8_lossy(&body);
    let xml_body = |s: String| {
        (
            [(header::CONTENT_TYPE, "text/xml; charset=utf-8")],
            s,
        )
            .into_response()
    };
    let (method, params) = match crate::xmlrpc::parse_method_call(&xml) {
        Ok(t) => t,
        Err(e) => {
            return xml_body(crate::xmlrpc::encode_fault(1, &e.to_string()));
        }
    };
    let req = RpcRequest {
        jsonrpc: None,
        id: None,
        method,
        params: Some(params),
    };
    match dispatch(&session, &req).await {
        Ok(result) => xml_body(crate::xmlrpc::encode_response(&result)),
        Err(e) => xml_body(crate::xmlrpc::encode_fault(1, &e.to_string())),
    }
}

async fn check_basic(
    session: &Arc<Session>,
    headers: &HeaderMap,
) -> std::result::Result<(), axum::response::Response> {
    let g = session.get_global_option().await;
    let user = g.get("rpc-user").and_then(|v| v.as_str()).unwrap_or("");
    if user.is_empty() {
        return Ok(());
    }
    let pass = g.get("rpc-passwd").and_then(|v| v.as_str()).unwrap_or("");
    let want = format!("Basic {}", b64_encode(format!("{user}:{pass}").as_bytes()));
    let got = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if got == want {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"aria2\"")],
            "Unauthorized",
        )
            .into_response())
    }
}

fn b64_encode(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i];
        let b1 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] } else { 0 };
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if i + 1 < data.len() {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if i + 2 < data.len() {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
        i += 3;
    }
    out
}

async fn dispatch(session: &Arc<Session>, req: &RpcRequest) -> crate::Result<Value> {
    let method = req.method.strip_prefix("aria2.").unwrap_or(&req.method);
    let params = req.params.clone().unwrap_or(Value::Array(vec![]));
    require_token(session, &params).await?;
    match req.method.as_str() {
        "system.listMethods" => Ok(json!(METHODS)),
        "system.listNotifications" => Ok(json!(["aria2.onDownloadStart", "aria2.onDownloadComplete", "aria2.onDownloadError"])),
        "system.multicall" => {
            let mut out = Vec::new();
            if let Some(arr) = params.as_array().and_then(|a| a.first()).and_then(|v| v.as_array()) {
                for call in arr {
                    let method = call.get("methodName").and_then(|v| v.as_str()).unwrap_or("");
                    let p = call.get("params").cloned();
                    let inner = RpcRequest {
                        jsonrpc: Some("2.0".into()),
                        id: None,
                        method: method.to_string(),
                        params: p,
                    };
                    match Box::pin(dispatch(session, &inner)).await {
                        Ok(r) => out.push(json!([r])),
                        Err(e) => out.push(json!({"faultCode": 1, "faultString": e.to_string()})),
                    }
                }
            }
            Ok(json!(out))
        }
        "aria2.getVersion" => Ok(json!({
            "version": crate::VERSION,
            "enabledFeatures": ["AsyncDNS", "BitTorrent", "HTTPS", "Message Digest", "Metalink", "XML-RPC", "SFTP"]
        })),
        "aria2.getGlobalStat" => Ok(session.global_stat().await),
        "aria2.shutdown" | "aria2.forceShutdown" => {
            tokio::spawn(async {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                std::process::exit(0);
            });
            Ok(json!("OK"))
        }
        "aria2.addUri" => {
            let (uris, extra) = parse_add_uri(&params)?;
            let gid = session.add_uri_and_start(uris, extra).await?;
            Ok(json!(gid.as_str()))
        }
        "aria2.addTorrent" => {
            let (torrent, uris, extra) = parse_add_torrent(&params)?;
            let gid = session.add_torrent_and_start(torrent, uris, extra).await?;
            Ok(json!(gid.as_str()))
        }
        "aria2.addMetalink" => {
            let (bytes, extra) = parse_add_metalink(&params)?;
            let gids = session.add_metalink_and_start(bytes, extra).await?;
            Ok(json!(gids.iter().map(|g| g.as_str()).collect::<Vec<_>>()))
        }
        "aria2.tellStatus" => {
            let gid = nth_str(&params, gid_index(&params))?;
            session.tell_status(gid).await
        }
        "aria2.tellActive" => Ok(json!(session.tell_active().await)),
        "aria2.tellWaiting" => Ok(json!(session.tell_waiting(0, 100).await)),
        "aria2.tellStopped" => Ok(json!(session.tell_stopped(0, 100).await)),
        "aria2.remove" => {
            let gid = nth_str(&params, gid_index(&params))?;
            Ok(json!(session.remove(gid).await?))
        }
        "aria2.forceRemove" => {
            let gid = nth_str(&params, gid_index(&params))?;
            Ok(json!(session.force_remove(gid).await?))
        }
        "aria2.removeDownloadResult" => {
            let gid = nth_str(&params, gid_index(&params))?;
            Ok(json!(session.remove_download_result(gid).await?))
        }
        "aria2.purgeDownloadResult" => Ok(json!(session.purge_download_result().await?)),
        "aria2.pause" | "aria2.forcePause" => {
            let gid = nth_str(&params, gid_index(&params))?;
            Ok(json!(session.pause(gid).await?))
        }
        "aria2.pauseAll" | "aria2.forcePauseAll" => Ok(json!(session.pause_all().await?)),
        "aria2.unpause" => {
            let gid = nth_str(&params, gid_index(&params))?;
            Ok(json!(session.unpause(gid).await?))
        }
        "aria2.unpauseAll" => Ok(json!(session.unpause_all().await?)),
        "aria2.getUris" => {
            let gid = nth_str(&params, gid_index(&params))?;
            session.get_uris(gid).await
        }
        "aria2.changeUri" => {
            let i = gid_index(&params);
            let gid = nth_str(&params, i)?;
            let file_index = nth_i64(&params, i + 1)?;
            let del = nth_str_vec(&params, i + 2)?;
            let add = nth_str_vec(&params, i + 3)?;
            let position = nth_i64_opt(&params, i + 4);
            let (deleted, added) = session
                .change_uri(gid, file_index, del, add, position)
                .await?;
            Ok(json!([deleted, added]))
        }
        "aria2.changePosition" => {
            let i = gid_index(&params);
            let gid = nth_str(&params, i)?;
            let pos = nth_i64(&params, i + 1)?;
            let how = params
                .as_array()
                .and_then(|a| a.get(i + 2))
                .and_then(|v| v.as_str())
                .unwrap_or("POS_SET");
            let n = session.change_position(gid, pos, how).await?;
            Ok(json!(n))
        }
        "aria2.getFiles" => {
            let gid = nth_str(&params, gid_index(&params))?;
            session.get_files(gid).await
        }
        "aria2.getPeers" => {
            let gid = nth_str(&params, gid_index(&params))?;
            session.get_peers(gid).await
        }
        "aria2.getServers" => {
            let gid = nth_str(&params, gid_index(&params))?;
            session.get_servers(gid).await
        }
        "aria2.getOption" => {
            let gid = nth_str(&params, gid_index(&params))?;
            session.get_option(gid).await
        }
        "aria2.changeOption" => {
            let i = gid_index(&params);
            let gid = nth_str(&params, i)?;
            let extra = parse_option_at(&params, i + 1)?;
            Ok(json!(session.change_option(gid, extra).await?))
        }
        "aria2.getGlobalOption" => Ok(session.get_global_option().await),
        "aria2.changeGlobalOption" => {
            let extra = parse_option_at(&params, gid_index(&params))?;
            Ok(json!(session.change_global_option(extra).await?))
        }
        "aria2.getSessionInfo" => Ok(session.get_session_info()),
        "aria2.saveSession" => Ok(json!(session.save_session().await?)),
        "aria2.tellRoomPeers" => Ok(json!(session.tell_room_peers().await)),
        "aria2.getRoomFiles" => Ok(json!(session.get_room_files().await)),
        "aria2.copyFromRoom" => {
            let i = gid_index(&params);
            let addr = nth_str(&params, i)?;
            let port = match params.as_array().and_then(|a| a.get(i + 1)) {
                Some(Value::Number(n)) => n.as_u64().unwrap_or(0) as u16,
                Some(Value::String(s)) => s.parse().unwrap_or(0),
                _ => 0,
            };
            let rel = nth_str(&params, i + 2)?;
            let deep = params
                .as_array()
                .and_then(|a| a.get(i + 3))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let gids = session.copy_from_room(addr, port, rel, deep).await?;
            Ok(json!(gids))
        }
        _ => Err(crate::Error::Rpc(format!("unknown method {method}"))),
    }
}

async fn require_token(session: &Arc<Session>, params: &Value) -> crate::Result<()> {
    let g = session.get_global_option().await;
    let secret = g
        .get("rpc-secret")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if secret.is_empty() {
        return Ok(());
    }
    let got = params
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str());
    let want = format!("token:{secret}");
    if got == Some(want.as_str()) {
        Ok(())
    } else {
        Err(crate::Error::Rpc("Unauthorized".into()))
    }
}

fn gid_index(params: &Value) -> usize {
    // token is params[0] when it looks like "token:..."
    if let Some(s) = params.as_array().and_then(|a| a.first()).and_then(|v| v.as_str()) {
        if s.starts_with("token:") {
            return 1;
        }
    }
    0
}

fn nth_str(params: &Value, i: usize) -> crate::Result<&str> {
    params
        .as_array()
        .and_then(|a| a.get(i))
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::Error::Rpc("missing gid".into()))
}

fn nth_i64(params: &Value, i: usize) -> crate::Result<i64> {
    let v = params
        .as_array()
        .and_then(|a| a.get(i))
        .ok_or_else(|| crate::Error::Rpc("missing fileIndex".into()))?;
    if let Some(n) = v.as_i64() {
        return Ok(n);
    }
    if let Some(s) = v.as_str() {
        return s
            .parse()
            .map_err(|_| crate::Error::Rpc("fileIndex".into()));
    }
    Err(crate::Error::Rpc("fileIndex".into()))
}

fn nth_i64_opt(params: &Value, i: usize) -> Option<i64> {
    nth_i64(params, i).ok()
}

fn nth_str_vec(params: &Value, i: usize) -> crate::Result<Vec<String>> {
    params
        .as_array()
        .and_then(|a| a.get(i))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .ok_or_else(|| crate::Error::Rpc("uris array".into()))
}

fn parse_add_uri(params: &Value) -> crate::Result<(Vec<String>, OptionSet)> {
    let arr = params.as_array().ok_or_else(|| crate::Error::Rpc("params".into()))?;
    let mut i = 0;
    if arr.first().and_then(|v| v.as_str()).is_some_and(|s| s.starts_with("token:")) {
        i = 1;
    }
    let uris = arr
        .get(i)
        .and_then(|v| v.as_array())
        .ok_or_else(|| crate::Error::Rpc("uris".into()))?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    let mut extra = OptionSet::new();
    if let Some(obj) = arr.get(i + 1).and_then(|v| v.as_object()) {
        for (k, v) in obj {
            extra.set(k, v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string()));
        }
    }
    Ok((uris, extra))
}

fn parse_add_torrent(params: &Value) -> crate::Result<(Vec<u8>, Vec<String>, OptionSet)> {
    let arr = params.as_array().ok_or_else(|| crate::Error::Rpc("params".into()))?;
    let mut i = 0;
    if arr
        .first()
        .and_then(|v| v.as_str())
        .is_some_and(|s| s.starts_with("token:"))
    {
        i = 1;
    }
    let b64 = arr
        .get(i)
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::Error::Rpc("torrent".into()))?;
    let torrent = crate::bt::b64_decode(b64)?;
    i += 1;
    let mut uris = Vec::new();
    if let Some(a) = arr.get(i).and_then(|v| v.as_array()) {
        uris = a
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        i += 1;
    }
    let mut extra = OptionSet::new();
    if let Some(obj) = arr.get(i).and_then(|v| v.as_object()) {
        for (k, v) in obj {
            extra.set(
                k,
                v.as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| v.to_string()),
            );
        }
    }
    Ok((torrent, uris, extra))
}

fn parse_add_metalink(params: &Value) -> crate::Result<(Vec<u8>, OptionSet)> {
    let arr = params.as_array().ok_or_else(|| crate::Error::Rpc("params".into()))?;
    let mut i = 0;
    if arr
        .first()
        .and_then(|v| v.as_str())
        .is_some_and(|s| s.starts_with("token:"))
    {
        i = 1;
    }
    let b64 = arr
        .get(i)
        .and_then(|v| v.as_str())
        .ok_or_else(|| crate::Error::Rpc("metalink".into()))?;
    let bytes = crate::bt::b64_decode(b64)?;
    let mut extra = OptionSet::new();
    if let Some(obj) = arr.get(i + 1).and_then(|v| v.as_object()) {
        for (k, v) in obj {
            extra.set(
                k,
                v.as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| v.to_string()),
            );
        }
    }
    Ok((bytes, extra))
}

fn parse_option_at(params: &Value, i: usize) -> crate::Result<OptionSet> {
    let mut extra = OptionSet::new();
    let Some(obj) = params.as_array().and_then(|a| a.get(i)).and_then(|v| v.as_object()) else {
        return Err(crate::Error::Rpc("options object".into()));
    };
    for (k, v) in obj {
        extra.set(
            k,
            v.as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| v.to_string()),
        );
    }
    Ok(extra)
}
