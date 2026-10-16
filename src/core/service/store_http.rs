//! E36: bootstrap HTTP belongs to the foreground. The CLI only requests a
//! listener over authenticated LocalTransport; it never opens a database.
use super::store_inputs::StoreInputs;
use ctox_store::{
    ipc::{self, JobKind, Request},
    JobHandle, JobOptions, StoreError,
};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::{Component, Path},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};
use tiny_http::{Header, Method, Response, Server};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Listen {
    addr: SocketAddr,
}

struct Listener {
    addr: SocketAddr,
    server: Arc<Server>,
    workers: Vec<thread::JoinHandle<()>>,
}
pub(crate) struct HttpSurface {
    inputs: Arc<StoreInputs>,
    closing: Arc<AtomicBool>,
    listeners: Mutex<Vec<Listener>>,
}
impl HttpSurface {
    pub fn new(inputs: Arc<StoreInputs>) -> Arc<Self> {
        Arc::new(Self {
            inputs,
            closing: Arc::new(AtomicBool::new(false)),
            listeners: Mutex::new(Vec::new()),
        })
    }
    /// Runs only on a blocking/IPC worker. Admission and thread count are bounded.
    pub fn listen(&self, addr: SocketAddr) -> Result<SocketAddr, StoreError> {
        self.inputs.instance_signer()?;
        let mut listeners = self.listeners.lock().unwrap_or_else(|p| p.into_inner());
        if self.closing.load(Ordering::Acquire) {
            return Err(StoreError::Closed);
        }
        if let Some(listener) = listeners.iter().find(|l| l.addr == addr) {
            return Ok(listener.addr);
        }
        if listeners.len() >= 8 {
            return Err(StoreError::Busy);
        }
        // Resolving the selected release is filesystem work, not SQLite work.
        let app =
            crate::business_os::resolve_business_os_app_root(&self.inputs.root).map_err(|_| {
                StoreError::Invalid("no verified Business OS bootstrap installed".into())
            })?;
        let app = std::fs::canonicalize(app)?;
        self.inputs.instance_signer()?;
        let server = Arc::new(
            Server::http(addr)
                .map_err(|_| StoreError::Invalid("HTTP address unavailable".into()))?,
        );
        let bound = server
            .server_addr()
            .to_ip()
            .ok_or_else(|| StoreError::Invalid("HTTP socket unavailable".into()))?;
        let mut listener = Listener {
            addr: bound,
            server: server.clone(),
            workers: Vec::new(),
        };
        for index in 0..4 {
            let server = server.clone();
            let inputs = self.inputs.clone();
            let closing = self.closing.clone();
            let app = app.clone();
            match thread::Builder::new()
                .name(format!("ctox-bootstrap-{index}"))
                .spawn(move || {
                    while !closing.load(Ordering::Acquire) {
                        match server.recv() {
                            Ok(request) => {
                                let result = answer(&inputs, &app, request.url(), request.method());
                                let (status, mime, body) = result.unwrap_or((
                                    503,
                                    "text/plain",
                                    b"bootstrap unavailable".to_vec(),
                                ));
                                let mut response =
                                    Response::from_data(body).with_status_code(status);
                                response
                                    .add_header(Header::from_bytes("Content-Type", mime).unwrap());
                                response.add_header(
                                    Header::from_bytes("Cache-Control", "no-store").unwrap(),
                                );
                                response.add_header(
                                    Header::from_bytes("X-Content-Type-Options", "nosniff")
                                        .unwrap(),
                                );
                                response.add_header(Header::from_bytes("Content-Security-Policy", "default-src 'self'; script-src 'self' blob:; connect-src 'self' wss:; object-src 'none'; base-uri 'self'").unwrap());
                                let _ = request.respond(response);
                            }
                            Err(_) => break,
                        }
                    }
                }) {
                Ok(worker) => listener.workers.push(worker),
                Err(error) => {
                    // Retain partial startup custody for the ordinary drain.
                    listeners.push(listener);
                    return Err(error.into());
                }
            }
        }
        listeners.push(listener);
        Ok(bound)
    }
    pub fn dispatch(
        self: &Arc<Self>,
        request: Request,
        options: JobOptions,
    ) -> Result<JobHandle<Vec<u8>>, StoreError> {
        if request.kind != JobKind::Read {
            return Err(StoreError::Invalid(
                "HTTP listener is a control operation".into(),
            ));
        }
        let listen: Listen = serde_json::from_slice(&request.payload)
            .map_err(|_| StoreError::Invalid("invalid HTTP listener".into()))?;
        options.check()?;
        let surface = self.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        self.inputs.runtime.spawn_blocking(move || {
            let result = options
                .check()
                .and_then(|_| surface.listen(listen.addr))
                .and_then(|addr| {
                    serde_json::to_vec(&Listen { addr })
                        .map_err(|_| StoreError::Invalid("HTTP result encoding".into()))
                });
            let _ = send.send(result);
        });
        Ok(JobHandle::from_receiver(receive))
    }
    pub async fn shutdown(self: &Arc<Self>) -> Result<(), StoreError> {
        self.closing.store(true, Ordering::Release);
        let surface = self.clone();
        tokio::task::spawn_blocking(move || {
            // listen() retains this mutex while verifying a selected release.
            // Waiting for that filesystem work must not block a Tokio worker.
            let listeners =
                std::mem::take(&mut *surface.listeners.lock().unwrap_or_else(|p| p.into_inner()));
            for listener in listeners {
                for _ in &listener.workers {
                    listener.server.unblock();
                }
                for worker in listener.workers {
                    worker.join().map_err(|_| StoreError::Panicked)?;
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| StoreError::Panicked)?
    }
}
impl Drop for HttpSurface {
    fn drop(&mut self) {
        self.closing.store(true, Ordering::Release);
        for listener in self.listeners.get_mut().unwrap_or_else(|p| p.into_inner()) {
            for _ in &listener.workers {
                listener.server.unblock();
            }
        }
    }
}

fn answer(
    inputs: &StoreInputs,
    app: &Path,
    url: &str,
    method: &Method,
) -> Result<(u16, &'static str, Vec<u8>), StoreError> {
    // This public surface supplies no actor, session cookie, grants or records.
    // Admin and non-admin URLs receive the same bootstrap. WebRTC authenticates.
    let signer = inputs.instance_signer()?;
    if !matches!(method, Method::Get | Method::Head) {
        return Ok((405, "text/plain", Vec::new()));
    }
    let path = url.split('?').next().unwrap_or(url);
    if path == "/api/business-os/bootstrap" {
        return Ok((200, "application/json", serde_json::to_vec(&serde_json::json!({"instance_id":signer.instance_id(),"instance_public_key":signer.key_id(),"protocol_ver":4})).map_err(|_| StoreError::Invalid("bootstrap encoding".into()))?));
    }
    if path.starts_with("/api/") {
        return Ok((404, "text/plain", Vec::new()));
    }
    // E38.1: the active slot is the bootstrap root. Never accidentally serve
    // its legacy V2 index. Business data remains exclusively on the V4 stream.
    let relative = match path {
        "/" | "/index.html" => "v4/index.html",
        "/sw.js" => "v4/sw.js",
        path if path.starts_with("/kernel/") => &path[1..],
        path if path.starts_with("/business-os/") => &path["/business-os/".len()..],
        _ => return Ok((404, "text/plain", Vec::new())),
    };
    // E38.2 excludes test-only kernel fixtures even through the slot alias.
    if relative.starts_with("kernel/")
        && relative
            .split('/')
            .any(|part| matches!(part, "tests" | "testkit") || part.starts_with("fake-core"))
    {
        return Ok((404, "text/plain", Vec::new()));
    }
    if Path::new(relative)
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
        || relative.contains('%')
        || relative.contains('\\')
    {
        return Ok((404, "text/plain", Vec::new()));
    }
    let target = match std::fs::canonicalize(app.join(relative)) {
        Ok(path) if path.starts_with(app) && path.is_file() => path,
        _ => return Ok((404, "text/plain", Vec::new())),
    };
    let mime = match target.extension().and_then(|e| e.to_str()) {
        Some("js" | "mjs") => "text/javascript",
        Some("html") => "text/html",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    };
    if target.metadata()?.len() > 1024 * 1024 {
        return Ok((413, "text/plain", Vec::new()));
    }
    Ok((
        200,
        mime,
        if method == &Method::Head {
            Vec::new()
        } else {
            std::fs::read(target)?
        },
    ))
}

pub(crate) fn serve(root: &Path, addr: &str) -> anyhow::Result<()> {
    let listen = Listen {
        addr: addr.parse()?,
    };
    let request = Request {
        request_id: 1,
        kind: JobKind::Read,
        method: "http.listen".into(),
        payload: serde_json::to_vec(&listen)?,
    };
    let job = ipc::request(
        super::queue_store::endpoint(root),
        request,
        JobOptions::default(),
    )?;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    crate::runtime::shared()?.spawn(async move {
        let _ = send.send(job.await);
    });
    let result: Listen = serde_json::from_slice(&receive.recv()??)?;
    println!(
        "CTOX Business OS listening on http://{} (owned by foreground daemon)",
        result.addr
    );
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::Duration;
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn foreground_http_reuses_owned_stores_and_cli_ipc() -> anyhow::Result<()> {
        let _scope = super::super::store_startup::isolate_test_roots().await;
        anyhow::ensure!(
            std::env::var_os("CTOX_STATE_ROOT").is_none(),
            "configured profile forbidden in HTTP contract"
        );
        let root = tempfile::Builder::new().prefix("c7h-").tempdir_in("/tmp")?;
        let app = root.path().join("business-os");
        std::fs::create_dir(&app)?;
        std::fs::write(
            app.join("index.html"),
            "<!doctype html><title>bootstrap fixture</title>",
        )?;
        std::fs::create_dir_all(app.join("v4"))?;
        std::fs::create_dir_all(app.join("kernel/vendor"))?;
        std::fs::create_dir_all(app.join("kernel/tests"))?;
        std::fs::create_dir_all(app.join("assets"))?;
        for (path, body) in [
            (
                "v4/index.html",
                "<!doctype html><base href=/business-os/><title>V4</title>",
            ),
            ("v4/sw.js", "// V4 service worker"),
            ("v4/boot.js", "// V4 boot"),
            ("kernel/boot.mjs", "// kernel boot"),
            ("kernel/vendor/hello.mjs", "// kernel vendor"),
            ("kernel/tests/private.mjs", "// forbidden fixture"),
            ("assets/ctox-app-icon.png", "icon fixture"),
        ] {
            std::fs::write(app.join(path), body)?;
        }
        let mut owner = super::super::store_startup::Foreground::start(
            root.path().to_owned(),
            tokio::runtime::Handle::current(),
        )
        .await?;
        assert!(matches!(
            ctox_store::registry::Registry
                .open(owner.inputs.sync.writer.db_id().clone(), Default::default()),
            Err(StoreError::AlreadyOwned)
        ));
        let request = Request {
            request_id: 7,
            kind: JobKind::Read,
            method: "http.listen".into(),
            payload: serde_json::to_vec(&Listen {
                addr: "127.0.0.1:0".parse()?,
            })?,
        };
        let bytes = ipc::request(
            super::super::queue_store::endpoint(root.path()),
            request,
            Default::default(),
        )?
        .await?;
        let listen: Listen = serde_json::from_slice(&bytes)?;
        let child_root = root.path().to_owned();
        let child_addr = listen.addr;
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let child = std::process::Command::new(std::env::current_exe()?)
                .args([
                    "service::store_http::tests::http_cli_process_probe",
                    "--exact",
                    "--ignored",
                    "--nocapture",
                ])
                .env("CTOX_C7_HTTP_ROOT", child_root)
                .env("CTOX_C7_HTTP_ADDR", child_addr.to_string())
                .output()?;
            assert!(
                child.status.success(),
                "separate serve process failed beside daemon: {}",
                String::from_utf8_lossy(&child.stdout)
            );
            Ok(())
        })
        .await??;
        let identity = owner.inputs.instance_signer()?;
        let expected = (
            identity.instance_id().to_owned(),
            identity.key_id().to_owned(),
        );
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let client = ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(3))
                .build();
            let get = |path: &str| -> anyhow::Result<ureq::Response> {
                match client.get(&format!("http://{}{path}", listen.addr)).call() {
                    Ok(response) | Err(ureq::Error::Status(_, response)) => Ok(response),
                    Err(error) => Err(error.into()),
                }
            };
            for url in ["/", "/?role=admin", "/?role=user"] {
                let response = get(url)?;
                assert_eq!(
                    response.status(),
                    200,
                    "HTTP failed beside foreground owner: {url}"
                );
                assert_eq!(
                    response
                        .header("Content-Security-Policy")
                        .unwrap()
                        .split("base-uri ")
                        .nth(1),
                    Some("'self'")
                );
                assert!(
                    response.into_string()?.contains("<title>V4</title>"),
                    "V2 index leaked into V4 entry"
                );
            }
            for (url, body) in [
                ("/sw.js?db=probe", "// V4 service worker"),
                ("/kernel/boot.mjs", "// kernel boot"),
                ("/kernel/vendor/hello.mjs", "// kernel vendor"),
                ("/business-os/v4/boot.js", "// V4 boot"),
                ("/business-os/assets/ctox-app-icon.png", "icon fixture"),
            ] {
                let response = get(url)?;
                assert_eq!(response.status(), 200, "missing E38 route: {url}");
                assert_eq!(response.into_string()?, body);
            }
            for url in [
                "/kernel/tests/private.mjs",
                "/business-os/kernel/tests/private.mjs",
                "/business-os/%2e%2e/index.html",
            ] {
                assert_eq!(get(url)?.status(), 404, "bootstrap path escape: {url}");
            }
            let bootstrap: serde_json::Value = get("/api/business-os/bootstrap")?.into_json()?;
            assert_eq!(bootstrap["instance_id"], expected.0);
            assert_eq!(bootstrap["instance_public_key"], expected.1);
            assert!(bootstrap.get("session").is_none());
            assert_eq!(get("/api/business-os/records")?.status(), 404);
            assert_eq!(get("/modules/private/index.js")?.status(), 404);
            Ok(())
        })
        .await??;
        owner.shutdown().await?;
        assert!(
            std::net::TcpStream::connect_timeout(&listen.addr, Duration::from_millis(100)).is_err(),
            "HTTP survived owner shutdown"
        );
        Ok(())
    }

    /// Executed only by the parent contract in a real second process.
    #[test]
    #[ignore]
    fn http_cli_process_probe() -> anyhow::Result<()> {
        let root = std::env::var_os("CTOX_C7_HTTP_ROOT")
            .ok_or_else(|| anyhow::anyhow!("parent contract root required"))?;
        let addr = std::env::var("CTOX_C7_HTTP_ADDR")?;
        serve(Path::new(&root), &addr)
    }
}
