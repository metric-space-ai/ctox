// Origin: CTOX
// License: Apache-2.0

use anyhow::Context;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tiny_http::Header;
use tiny_http::Method;
use tiny_http::Request;
use tiny_http::Response;
use tiny_http::Server;

use super::store;

#[derive(Debug, Clone)]
pub struct BusinessOsServeOptions {
    pub addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModuleManifest {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    entry: String,
    #[serde(default)]
    collections: Vec<String>,
    #[serde(default)]
    layout: Value,
}

pub fn serve_business_os(root: &Path, options: BusinessOsServeOptions) -> anyhow::Result<()> {
    let app_root = root.join("business-os");
    if !app_root.join("index.html").is_file() {
        anyhow::bail!(
            "native Business OS app is missing at {}",
            app_root.display()
        );
    }
    let _conn = store::open_store(root)?;
    let server = Server::http(&options.addr)
        .map_err(|err| anyhow::anyhow!("failed to bind Business OS server: {err}"))?;
    println!("CTOX Business OS listening on http://{}", options.addr);
    println!("Serving {}", app_root.display());
    for request in server.incoming_requests() {
        if let Err(err) = handle_request(root, &app_root, request) {
            eprintln!("[business-os] request failed: {err:#}");
        }
    }
    Ok(())
}

fn handle_request(root: &Path, app_root: &Path, mut request: Request) -> anyhow::Result<()> {
    let method = request.method().clone();
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("/");
    match (method.clone(), path) {
        (Method::Get, "/api/business-os/status") => {
            respond_json(request, &store::status(root)?)?;
        }
        (Method::Get, "/api/business-os/session") => {
            let auth_header = header_value(&request, "Authorization");
            let session_header = header_value(&request, "X-CTOX-Business-OS-Session");
            respond_json(
                request,
                &store::session(auth_header.as_deref(), session_header.as_deref()),
            )?;
        }
        (Method::Get, "/api/business-os/modules") => {
            respond_json(
                request,
                &serde_json::json!({
                    "ok": true,
                    "modules": load_module_manifests(app_root)?
                }),
            )?;
        }
        (Method::Get, "/api/business-os/sync/config") => {
            respond_json(request, &store::sync_config(root)?)?;
        }
        (Method::Post, "/api/business-os/commands") => {
            let body = read_json(&mut request)?;
            let command = serde_json::from_value(body)?;
            let accepted = store::record_command(root, command)?;
            respond_json_value(request, serde_json::to_value(accepted)?)?;
        }
        _ if method == Method::Post && path.starts_with("/api/business-os/rxdb/") => {
            let collection = path
                .trim_start_matches("/api/business-os/rxdb/")
                .trim_end_matches("/pull")
                .trim_end_matches("/push");
            if collection.is_empty() || collection.contains('/') {
                respond_status(request, 400, "invalid collection")?;
            } else if path.ends_with("/pull") {
                respond_json_value(request, store::pull_collection(root, collection)?)?;
            } else if path.ends_with("/push") {
                let body = read_json(&mut request)?;
                respond_json_value(request, store::push_collection(root, collection, body)?)?;
            } else {
                respond_status(request, 404, "not found")?;
            }
        }
        _ if method == Method::Get => serve_static(app_root, request, path)?,
        _ => respond_status(request, 405, "method not allowed")?,
    }
    Ok(())
}

fn header_value(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str().to_owned())
}

fn load_module_manifests(app_root: &Path) -> anyhow::Result<Vec<ModuleManifest>> {
    let modules_root = app_root.join("modules");
    let mut manifests = Vec::new();
    if !modules_root.is_dir() {
        return Ok(manifests);
    }
    for entry in fs::read_dir(&modules_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("module.json");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read module manifest {}", path.display()))?;
        let mut manifest: ModuleManifest = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse module manifest {}", path.display()))?;
        if manifest.entry.is_empty() {
            manifest.entry = format!("modules/{}/index.html", manifest.id);
        }
        manifests.push(manifest);
    }
    manifests.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(manifests)
}

fn serve_static(app_root: &Path, request: Request, path: &str) -> anyhow::Result<()> {
    let rel = if path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    if rel
        .split('/')
        .any(|part| part == ".." || part.starts_with('.'))
    {
        return respond_status(request, 403, "forbidden");
    }
    let file = app_root.join(rel);
    let target = if file.is_dir() {
        file.join("index.html")
    } else {
        file
    };
    if !target.is_file() {
        return respond_status(request, 404, "not found");
    }
    let bytes = fs::read(&target)?;
    let mime = mime_for(&target);
    let mut response = Response::from_data(bytes);
    response.add_header(Header::from_bytes("Content-Type", mime.as_bytes()).unwrap());
    request.respond(response)?;
    Ok(())
}

fn read_json(request: &mut Request) -> anyhow::Result<Value> {
    let mut text = String::new();
    request.as_reader().read_to_string(&mut text)?;
    if text.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&text).context("invalid JSON request body")
}

fn respond_json<T: Serialize>(request: Request, value: &T) -> anyhow::Result<()> {
    respond_json_value(request, serde_json::to_value(value)?)
}

fn respond_json_value(request: Request, value: Value) -> anyhow::Result<()> {
    let body = serde_json::to_string_pretty(&value)?;
    let mut response = Response::from_string(body);
    response.add_header(Header::from_bytes("Content-Type", "application/json").unwrap());
    request.respond(response)?;
    Ok(())
}

fn respond_status(request: Request, status: u16, body: &str) -> anyhow::Result<()> {
    let response = Response::from_string(body.to_string()).with_status_code(status);
    request.respond(response)?;
    Ok(())
}

fn mime_for(path: &PathBuf) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}
