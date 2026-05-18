// Origin: CTOX
// License: Apache-2.0

use anyhow::Context;
use polars::prelude::*;
use rusqlite::Connection;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use sha2::Digest;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tiny_http::Header;
use tiny_http::Method;
use tiny_http::Request;
use tiny_http::Response;
use tiny_http::Server;

use super::store;

const CORE_MODULE_IDS: &[&str] = &["ctox", "documents", "knowledge", "matching"];

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
    #[serde(default)]
    source: String,
    #[serde(default)]
    core: bool,
    #[serde(default)]
    editable: bool,
    #[serde(default)]
    deletable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TemplateManifest {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    source_module: String,
    #[serde(default)]
    default_title: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct InstallTemplateRequest {
    template_id: String,
    #[serde(default)]
    module_id: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UpsertModuleRequest {
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

#[derive(Debug, Clone, Deserialize)]
struct DeleteModuleRequest {
    module_id: String,
}

pub fn serve_business_os(root: &Path, options: BusinessOsServeOptions) -> anyhow::Result<()> {
    let app_root = resolve_business_os_app_root(root);
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

fn resolve_business_os_app_root(root: &Path) -> PathBuf {
    [
        root.join("business-os"),
        root.join("src/apps/business-os"),
        root.join("archive/2026-05-18-cleanup/generated/business-os"),
    ]
    .into_iter()
    .find(|candidate| candidate.join("index.html").is_file())
    .unwrap_or_else(|| root.join("business-os"))
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
        (Method::Get, "/api/business-os/users") => {
            let session = request_session(&request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                respond_json_value(request, store::list_users(root, &session)?)?;
            }
        }
        (Method::Post, "/api/business-os/users") => {
            let session = request_session(&request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !session
                .user
                .as_ref()
                .map(|user| user.is_admin)
                .unwrap_or(false)
            {
                respond_status(request, 403, "admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let mutation = serde_json::from_value(body)?;
                respond_json_value(request, store::upsert_user(root, &session, mutation)?)?;
            }
        }
        (Method::Get, "/api/business-os/modules") => {
            let session = request_session(&request);
            respond_json(
                request,
                &serde_json::json!({
                    "ok": true,
                    "modules": load_module_manifests(app_root)?,
                    "governance": store::module_governance_map(root, &session)?
                }),
            )?;
        }
        (Method::Get, "/api/business-os/module-governance") => {
            let session = request_session(&request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                respond_json_value(request, store::module_governance_map(root, &session)?)?;
            }
        }
        (Method::Post, "/api/business-os/modules") => {
            let session = request_session(&request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                let body = read_json(&mut request)?;
                let mutation: UpsertModuleRequest = serde_json::from_value(body)?;
                if !store::session_can_modify_module(root, &session, &mutation.id)? {
                    respond_status(request, 403, "module modification rights required")?;
                    return Ok(());
                }
                let manifest = upsert_module_manifest(app_root, mutation)?;
                respond_json(
                    request,
                    &serde_json::json!({
                        "ok": true,
                        "module": manifest
                    }),
                )?;
            }
        }
        (Method::Get, "/api/business-os/module-layout") => {
            respond_json_value(request, load_module_layout(root)?)?;
        }
        (Method::Post, "/api/business-os/module-layout") => {
            let body = read_json(&mut request)?;
            save_module_layout(root, &body)?;
            respond_json(
                request,
                &serde_json::json!({
                    "ok": true,
                    "layout": body
                }),
            )?;
        }
        (Method::Get, "/api/business-os/templates") => {
            respond_json(
                request,
                &serde_json::json!({
                    "ok": true,
                    "templates": load_template_manifests(app_root)?
                }),
            )?;
        }
        (Method::Get, "/api/business-os/knowledge") => {
            respond_json_value(request, knowledge_index_payload(root)?)?;
        }
        (Method::Get, "/api/business-os/knowledge/document") => {
            let query = parse_query(&url);
            let id = query.get("id").map(String::as_str).unwrap_or("");
            respond_json_value(request, knowledge_document_payload(root, id)?)?;
        }
        (Method::Get, "/api/business-os/knowledge/dataframe/schema") => {
            let query = parse_query(&url);
            let id = query.get("id").map(String::as_str).unwrap_or("");
            respond_json_value(request, knowledge_dataframe_schema_payload(root, id)?)?;
        }
        (Method::Get, "/api/business-os/knowledge/dataframe/rows") => {
            let query = parse_query(&url);
            let id = query.get("id").map(String::as_str).unwrap_or("");
            let offset = parse_usize_query(&query, "offset", 0);
            let limit = parse_usize_query(&query, "limit", 120).clamp(1, 500);
            respond_json_value(
                request,
                knowledge_dataframe_rows_payload(root, id, offset, limit)?,
            )?;
        }
        (Method::Post, "/api/business-os/modules/install-template") => {
            let session = request_session(&request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else if !store::session_can_manage_all(&session) {
                respond_status(request, 403, "chef or admin role required")?;
            } else {
                let body = read_json(&mut request)?;
                let install = serde_json::from_value(body)?;
                let manifest = install_template_module(app_root, install)?;
                respond_json(
                    request,
                    &serde_json::json!({
                        "ok": true,
                        "module": manifest
                    }),
                )?;
            }
        }
        (Method::Post, "/api/business-os/modules/delete") => {
            let session = request_session(&request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                let body = read_json(&mut request)?;
                let delete: DeleteModuleRequest = serde_json::from_value(body)?;
                if !store::session_can_modify_module(root, &session, &delete.module_id)? {
                    respond_status(request, 403, "module modification rights required")?;
                    return Ok(());
                }
                delete_installed_module(app_root, root, delete)?;
                respond_json(
                    request,
                    &serde_json::json!({
                        "ok": true
                    }),
                )?;
            }
        }
        (Method::Post, "/api/business-os/modules/assign-founder") => {
            let session = request_session(&request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                let body = read_json(&mut request)?;
                let assignment = serde_json::from_value(body)?;
                respond_json_value(
                    request,
                    store::assign_module_founder(root, &session, assignment)?,
                )?;
            }
        }
        (Method::Post, "/api/business-os/modules/release") => {
            let session = request_session(&request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                let body = read_json(&mut request)?;
                let release = serde_json::from_value(body)?;
                respond_json_value(
                    request,
                    store::record_module_release(root, app_root, &session, release)?,
                )?;
            }
        }
        (Method::Post, "/api/business-os/modules/rollback") => {
            let session = request_session(&request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                let body = read_json(&mut request)?;
                let rollback = serde_json::from_value(body)?;
                respond_json_value(
                    request,
                    store::rollback_module_release(root, app_root, &session, rollback)?,
                )?;
            }
        }
        (Method::Post, "/api/business-os/reports") => {
            let session = request_session(&request);
            if !session.authenticated {
                respond_status(request, 401, "login required")?;
            } else {
                let body = read_json(&mut request)?;
                let report = serde_json::from_value(body)?;
                respond_json_value(request, store::record_report(root, &session, report)?)?;
            }
        }
        (Method::Get, "/api/business-os/sync/config") => {
            respond_json(request, &store::sync_config(root)?)?;
        }
        (Method::Get, "/api/business-os/ctox/harness-flow") => {
            respond_json_value(request, latest_harness_flow_payload(root))?;
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
                let body = read_json(&mut request).unwrap_or_else(|_| serde_json::json!({}));
                respond_json_value(request, store::pull_collection(root, collection, body)?)?;
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

fn request_session(request: &Request) -> store::BusinessOsSession {
    let auth_header = header_value(request, "Authorization");
    let session_header = header_value(request, "X-CTOX-Business-OS-Session");
    store::session(auth_header.as_deref(), session_header.as_deref())
}

fn latest_harness_flow_payload(root: &Path) -> Value {
    match crate::service::harness_flow::load_latest_flow(root) {
        Ok(flow) => {
            let ascii = crate::service::harness_flow::render_latest_ascii(root, 132)
                .unwrap_or_else(|err| format!("failed to render harness flow: {err}"));
            serde_json::json!({
                "ok": true,
                "mode": "ctox_core",
                "flow": flow,
                "ascii": ascii
            })
        }
        Err(err) => serde_json::json!({
            "ok": false,
            "mode": "ctox_core",
            "error": err.to_string()
        }),
    }
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
        return load_installed_module_manifests(app_root);
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
        if !is_core_module(&manifest.id) {
            continue;
        }
        if manifest.entry.is_empty() {
            manifest.entry = format!("modules/{}/index.html", manifest.id);
        }
        manifest.source = "core".to_owned();
        manifest.core = true;
        manifest.editable = true;
        manifest.deletable = false;
        manifests.push(manifest);
    }
    manifests.extend(load_installed_module_manifests(app_root)?);
    manifests.sort_by(|a, b| match (a.id.as_str(), b.id.as_str()) {
        ("ctox", "ctox") => std::cmp::Ordering::Equal,
        ("ctox", _) => std::cmp::Ordering::Less,
        (_, "ctox") => std::cmp::Ordering::Greater,
        _ => a.title.cmp(&b.title).then_with(|| a.id.cmp(&b.id)),
    });
    Ok(manifests)
}

fn load_installed_module_manifests(app_root: &Path) -> anyhow::Result<Vec<ModuleManifest>> {
    let modules_root = app_root.join("installed-modules");
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
        if is_core_module(&manifest.id) {
            continue;
        }
        if manifest.entry.is_empty() {
            manifest.entry = format!("installed-modules/{}/index.html", manifest.id);
        }
        manifest.source = "installed".to_owned();
        manifest.core = false;
        manifest.editable = true;
        manifest.deletable = true;
        manifests.push(manifest);
    }
    Ok(manifests)
}

fn load_template_manifests(app_root: &Path) -> anyhow::Result<Vec<TemplateManifest>> {
    let templates_root = app_root.join("template-store");
    let mut templates = Vec::new();
    if !templates_root.is_dir() {
        return Ok(templates);
    }
    for entry in fs::read_dir(&templates_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("template.json");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read template manifest {}", path.display()))?;
        let template: TemplateManifest = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse template manifest {}", path.display()))?;
        templates.push(template);
    }
    templates.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.id.cmp(&b.id)));
    Ok(templates)
}

fn module_layout_path(root: &Path) -> PathBuf {
    root.join("runtime").join("business-os-module-layout.json")
}

fn load_module_layout(root: &Path) -> anyhow::Result<Value> {
    let path = module_layout_path(root);
    if !path.is_file() {
        return Ok(serde_json::json!({
            "ok": true,
            "version": 1,
            "labels": {},
            "ungrouped": [],
            "groups": []
        }));
    }
    let mut value: Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .with_context(|| format!("failed to read module layout {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse module layout {}", path.display()))?;
    if let Value::Object(map) = &mut value {
        map.insert("ok".to_owned(), Value::Bool(true));
    }
    Ok(value)
}

fn save_module_layout(root: &Path, layout: &Value) -> anyhow::Result<()> {
    let path = module_layout_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut clean = layout.clone();
    if let Value::Object(map) = &mut clean {
        map.remove("ok");
    }
    fs::write(&path, serde_json::to_vec_pretty(&clean)?)
        .with_context(|| format!("failed to write module layout {}", path.display()))?;
    Ok(())
}

fn install_template_module(
    app_root: &Path,
    request: InstallTemplateRequest,
) -> anyhow::Result<ModuleManifest> {
    let template_id = sanitize_slug(&request.template_id);
    if template_id.is_empty() {
        anyhow::bail!("template_id is required");
    }
    let template_path = app_root
        .join("template-store")
        .join(&template_id)
        .join("template.json");
    let text = fs::read_to_string(&template_path).with_context(|| {
        format!(
            "failed to read template manifest {}",
            template_path.display()
        )
    })?;
    let template: TemplateManifest = serde_json::from_str(&text).with_context(|| {
        format!(
            "failed to parse template manifest {}",
            template_path.display()
        )
    })?;
    let source_module = sanitize_slug(if template.source_module.is_empty() {
        &template.id
    } else {
        &template.source_module
    });
    let source = app_root.join("modules").join(&source_module);
    if !source.join("module.json").is_file() {
        anyhow::bail!("template source module `{source_module}` is missing");
    }
    let requested_id = sanitize_slug(if request.module_id.trim().is_empty() {
        if request.title.trim().is_empty() {
            &template.id
        } else {
            &request.title
        }
    } else {
        &request.module_id
    });
    let module_id = unique_module_id(app_root, &requested_id);
    let module_title = if request.title.trim().is_empty() {
        if template.default_title.trim().is_empty() {
            template.title.clone()
        } else {
            template.default_title.clone()
        }
    } else {
        request.title.trim().to_owned()
    };
    let target = app_root.join("installed-modules").join(&module_id);
    copy_dir_recursive(&source, &target)?;

    let manifest_path = target.join("module.json");
    let mut manifest_value: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )?;
    manifest_value["id"] = Value::String(module_id.clone());
    manifest_value["title"] = Value::String(module_title);
    manifest_value["entry"] = Value::String(format!("installed-modules/{module_id}/index.html"));
    manifest_value["template_id"] = Value::String(template.id);
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest_value)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let mut manifest: ModuleManifest = serde_json::from_value(manifest_value)?;
    manifest.source = "installed".to_owned();
    manifest.core = false;
    manifest.editable = true;
    manifest.deletable = true;
    Ok(manifest)
}

fn upsert_module_manifest(
    app_root: &Path,
    request: UpsertModuleRequest,
) -> anyhow::Result<ModuleManifest> {
    let module_id = sanitize_slug(&request.id);
    if module_id.is_empty() {
        anyhow::bail!("module id is required");
    }
    let title = request.title.trim();
    if title.is_empty() {
        anyhow::bail!("module title is required");
    }
    let is_core = is_core_module(&module_id);
    let target = if is_core {
        app_root.join("modules").join(&module_id)
    } else {
        app_root.join("installed-modules").join(&module_id)
    };
    let manifest_path = target.join("module.json");
    if !manifest_path.is_file() {
        create_blank_installed_module(app_root, &module_id, title, &request.description)?;
    }
    let mut manifest_value: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )?;
    manifest_value["id"] = Value::String(module_id.clone());
    manifest_value["title"] = Value::String(title.to_owned());
    manifest_value["description"] = Value::String(request.description.trim().to_owned());
    let entry = if is_core {
        format!("modules/{module_id}/index.html")
    } else if request.entry.trim().is_empty() {
        format!("installed-modules/{module_id}/index.html")
    } else {
        request.entry.trim().to_owned()
    };
    manifest_value["entry"] = Value::String(entry);
    manifest_value["collections"] = Value::Array(
        request
            .collections
            .into_iter()
            .map(|item| item.trim().to_owned())
            .filter(|item| !item.is_empty())
            .map(Value::String)
            .collect(),
    );
    if !request.layout.is_null() {
        manifest_value["layout"] = request.layout;
    }
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest_value)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let mut manifest: ModuleManifest = serde_json::from_value(manifest_value)?;
    manifest.source = if is_core { "core" } else { "installed" }.to_owned();
    manifest.core = is_core;
    manifest.editable = true;
    manifest.deletable = !is_core;
    Ok(manifest)
}

fn create_blank_installed_module(
    app_root: &Path,
    module_id: &str,
    title: &str,
    description: &str,
) -> anyhow::Result<()> {
    if is_core_module(module_id) {
        anyhow::bail!("core module does not exist: {module_id}");
    }
    let target = app_root.join("installed-modules").join(module_id);
    if target.exists() {
        anyhow::bail!("target module already exists: {}", target.display());
    }
    fs::create_dir_all(&target)
        .with_context(|| format!("failed to create module dir {}", target.display()))?;
    let manifest = serde_json::json!({
        "id": module_id,
        "title": title,
        "description": description,
        "entry": format!("installed-modules/{module_id}/index.html"),
        "collections": ["business_commands"],
        "layout": {
            "shell": "pane",
            "center": "module workspace"
        }
    });
    fs::write(
        target.join("module.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    fs::write(
        target.join("index.html"),
        format!(
            "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\"><title>{}</title></head><body><div data-module-root></div></body></html>\n",
            html_escape(title)
        ),
    )?;
    fs::write(
        target.join("index.js"),
        format!(
            "export async function mount({{ host, module }}) {{\n  host.innerHTML = `<section class=\"blank-module\"><h1>${{module.title || '{}'}}</h1><p>${{module.description || 'Neues Business-OS Modul.'}}</p></section>`;\n  return () => {{}};\n}}\n",
            js_escape(title)
        ),
    )?;
    fs::write(target.join("schema.js"), "export const collections = [];\n")?;
    Ok(())
}

fn delete_installed_module(
    app_root: &Path,
    root: &Path,
    request: DeleteModuleRequest,
) -> anyhow::Result<()> {
    let module_id = sanitize_slug(&request.module_id);
    if module_id.is_empty() {
        anyhow::bail!("module id is required");
    }
    if is_core_module(&module_id) {
        anyhow::bail!("core modules cannot be deleted");
    }
    let target = app_root.join("installed-modules").join(&module_id);
    if !target.is_dir() {
        anyhow::bail!("installed module not found: {module_id}");
    }
    fs::remove_dir_all(&target)
        .with_context(|| format!("failed to delete module dir {}", target.display()))?;
    let mut layout = load_module_layout(root)?;
    remove_module_from_layout_value(&mut layout, &module_id);
    save_module_layout(root, &layout)?;
    Ok(())
}

fn remove_module_from_layout_value(layout: &mut Value, module_id: &str) {
    let Some(map) = layout.as_object_mut() else {
        return;
    };
    if let Some(Value::Array(items)) = map.get_mut("ungrouped") {
        items.retain(|item| item.as_str() != Some(module_id));
    }
    if let Some(Value::Array(groups)) = map.get_mut("groups") {
        for group in groups {
            if let Some(Value::Array(items)) = group.get_mut("items") {
                items.retain(|item| item.as_str() != Some(module_id));
            }
        }
    }
    if let Some(Value::Object(labels)) = map.get_mut("labels") {
        labels.remove(module_id);
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn js_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn unique_module_id(app_root: &Path, requested_id: &str) -> String {
    let base = if requested_id.is_empty() {
        "module".to_owned()
    } else if is_core_module(requested_id) {
        format!("{requested_id}-copy")
    } else {
        requested_id.to_owned()
    };
    let installed_root = app_root.join("installed-modules");
    if !installed_root.join(&base).exists() {
        return base;
    }
    for index in 2..1000 {
        let candidate = format!("{base}-{index}");
        if !installed_root.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{base}-{}", uuid::Uuid::new_v4())
}

fn is_core_module(id: &str) -> bool {
    CORE_MODULE_IDS.iter().any(|core| id == *core)
}

fn sanitize_slug(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_owned()
}

fn copy_dir_recursive(source: &Path, target: &Path) -> anyhow::Result<()> {
    if target.exists() {
        anyhow::bail!("target module already exists: {}", target.display());
    }
    fs::create_dir_all(target)
        .with_context(|| format!("failed to create module dir {}", target.display()))?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if file_type.is_file() {
            fs::copy(&from, &to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
        }
    }
    Ok(())
}

fn knowledge_index_payload(root: &Path) -> anyhow::Result<Value> {
    let mut items = Vec::new();
    let mut runbooks = Vec::new();
    let mut tables = Vec::new();
    let sqlite_path = ctox_sqlite_path(root);

    if sqlite_path.is_file() {
        let conn = Connection::open(&sqlite_path).with_context(|| {
            format!("failed to open CTOX knowledge DB {}", sqlite_path.display())
        })?;

        let mut stmt = conn.prepare(
            "SELECT b.skill_id, b.skill_name, b.class, b.state, b.description, b.source_path,
                    b.cluster, b.updated_at, COALESCE(f.file_count, 0) AS file_count
               FROM ctox_skill_bundles b
               LEFT JOIN (
                 SELECT skill_id, COUNT(*) AS file_count FROM ctox_skill_files GROUP BY skill_id
               ) f ON f.skill_id = b.skill_id
              ORDER BY b.updated_at DESC, b.skill_name
              LIMIT 240",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let skill_id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let class_name: String = row.get(2)?;
            let state: String = row.get(3)?;
            let summary: String = row.get(4)?;
            let source_path: Option<String> = row.get(5)?;
            let cluster: String = row.get(6)?;
            let updated_at: String = row.get(7)?;
            let file_count: i64 = row.get(8)?;
            items.push(serde_json::json!({
                "id": format!("skill:{skill_id}"),
                "kind": "skill",
                "title": title,
                "subtitle": format!("{class_name} · {state} · {cluster}"),
                "summary": summary,
                "source_path": source_path,
                "updated_at": updated_at,
                "file_count": file_count,
                "has_table": false
            }));
        }

        let mut stmt = conn.prepare(
            "SELECT skillbook_id, title, status, summary, linked_runbooks_json, updated_at
               FROM knowledge_skillbooks
              ORDER BY updated_at DESC, title
              LIMIT 160",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let status: String = row.get(2)?;
            let summary: String = row.get(3)?;
            let linked_runbooks_json: String = row.get(4)?;
            let updated_at: String = row.get(5)?;
            let linked_runbook_ids = serde_json::from_str::<Value>(&linked_runbooks_json)
                .unwrap_or_else(|_| serde_json::json!([]));
            items.push(serde_json::json!({
                "id": format!("skillbook:{id}"),
                "kind": "skillbook",
                "title": title,
                "subtitle": format!("Skillbook · {status}"),
                "summary": summary,
                "linked_runbook_ids": linked_runbook_ids,
                "linked_runbooks_json": linked_runbooks_json,
                "updated_at": updated_at,
                "file_count": 1,
                "has_table": false
            }));
        }

        let mut stmt = conn.prepare(
            "SELECT runbook_id, skillbook_id, title, status, summary, problem_domain, updated_at
               FROM knowledge_runbooks
              ORDER BY updated_at DESC, title
              LIMIT 220",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let skillbook_id: String = row.get(1)?;
            let title: String = row.get(2)?;
            let status: String = row.get(3)?;
            let summary: String = row.get(4)?;
            let domain: String = row.get(5)?;
            let updated_at: String = row.get(6)?;
            let runbook = serde_json::json!({
                "id": format!("runbook:{id}"),
                "runbook_id": id,
                "skillbook_id": skillbook_id,
                "title": title,
                "status": status,
                "summary": summary,
                "problem_domain": domain,
                "updated_at": updated_at
            });
            items.push(serde_json::json!({
                "id": runbook["id"],
                "kind": "runbook",
                "runbook_id": runbook["runbook_id"],
                "skillbook_id": runbook["skillbook_id"],
                "title": runbook["title"],
                "subtitle": format!("Runbook · {} · {}", runbook["status"].as_str().unwrap_or(""), runbook["problem_domain"].as_str().unwrap_or("")),
                "summary": runbook["summary"],
                "problem_domain": runbook["problem_domain"],
                "updated_at": runbook["updated_at"],
                "file_count": 1,
                "has_table": false
            }));
            runbooks.push(runbook);
        }

        let mut stmt = conn.prepare(
            "SELECT table_id, domain, table_key, source_system, title, description, parquet_path,
                    row_count, bytes, updated_at
               FROM knowledge_data_tables
              WHERE archived_at IS NULL
              ORDER BY updated_at DESC, title
              LIMIT 160",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let table_id: String = row.get(0)?;
            let domain: String = row.get(1)?;
            let table_key: String = row.get(2)?;
            let source_system: String = row.get(3)?;
            let title: String = row.get(4)?;
            let description: String = row.get(5)?;
            let parquet_path: String = row.get(6)?;
            let row_count: i64 = row.get(7)?;
            let bytes: i64 = row.get(8)?;
            let updated_at: String = row.get(9)?;
            let id = format!("table:{table_id}");
            let table = serde_json::json!({
                "id": id,
                "kind": "dataframe",
                "title": title,
                "domain": domain,
                "table_key": table_key,
                "source_system": source_system,
                "description": description,
                "parquet_path": parquet_path,
                "row_count": row_count,
                "bytes": bytes,
                "updated_at": updated_at
            });
            items.push(serde_json::json!({
                "id": table["id"],
                "kind": "dataframe",
                "title": table["title"],
                "subtitle": format!("{} · {} rows", table["domain"].as_str().unwrap_or("data"), row_count),
                "summary": table["description"],
                "updated_at": table["updated_at"],
                "file_count": 1,
                "has_table": true
            }));
            tables.push(table);
        }
    }

    for table in scan_runtime_parquet_tables(root)? {
        let id = table["id"].as_str().unwrap_or_default().to_owned();
        let parquet_path = table["parquet_path"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if tables.iter().any(|existing| {
            existing["id"].as_str() == Some(id.as_str())
                || existing["parquet_path"].as_str() == Some(parquet_path.as_str())
        }) {
            continue;
        }
        items.push(serde_json::json!({
            "id": table["id"],
            "kind": "dataframe",
            "title": table["title"],
            "subtitle": table["subtitle"],
            "summary": table["description"],
            "updated_at": table["updated_at"],
            "file_count": 1,
            "has_table": true
        }));
        tables.push(table);
    }

    if runbooks.is_empty() {
        let runbook = serde_json::json!({
            "id": "runbook:knowledge-runtime-maintenance",
            "runbook_id": "knowledge-runtime-maintenance",
            "skillbook_id": "native-business-os-knowledge",
            "title": "Knowledge Runtime Maintenance",
            "status": "draft",
            "summary": "Operatives Standard-Runbook zum Prüfen, Aktualisieren und Anwenden von CTOX Knowledge.",
            "problem_domain": "knowledge",
            "updated_at": ""
        });
        items.push(serde_json::json!({
            "id": runbook["id"],
            "kind": "runbook",
            "title": runbook["title"],
            "subtitle": "Runbook · draft · knowledge",
            "summary": runbook["summary"],
            "updated_at": "",
            "file_count": 1,
            "has_table": false
        }));
        runbooks.push(runbook);
    }

    Ok(serde_json::json!({
        "ok": true,
        "source": if sqlite_path.is_file() { "ctox.sqlite3+runtime" } else { "runtime" },
        "items": items,
        "runbooks": runbooks,
        "tables": tables,
        "counts": {
            "items": items.len(),
            "runbooks": runbooks.len(),
            "tables": tables.len()
        }
    }))
}

fn knowledge_document_payload(root: &Path, id: &str) -> anyhow::Result<Value> {
    let markdown = if let Some(skill_id) = id.strip_prefix("skill:") {
        skill_markdown(root, skill_id)?
    } else if let Some(skillbook_id) = id.strip_prefix("skillbook:") {
        skillbook_markdown(root, skillbook_id)?
    } else if let Some(runbook_id) = id.strip_prefix("runbook:") {
        runbook_markdown(root, runbook_id)?
    } else if id.starts_with("table:") || id.starts_with("parquet:") {
        let table = resolve_parquet_table(root, id)?;
        format!(
            "# {}\n\n{}\n\n- Quelle: `{}`\n- Zeilen: {}\n- Bytes: {}\n\nDie Tabellenansicht lädt diese Daten windowed aus der CTOX-Polars-Schicht.",
            table.title,
            table.description,
            table.path.display(),
            table.row_count.map(|value| value.to_string()).unwrap_or_else(|| "unbekannt".to_owned()),
            table.bytes.map(|value| value.to_string()).unwrap_or_else(|| "unbekannt".to_owned())
        )
    } else {
        "# Knowledge\n\nKein Knowledge-Eintrag ausgewählt.".to_owned()
    };
    Ok(serde_json::json!({
        "ok": true,
        "id": id,
        "markdown": markdown
    }))
}

fn skill_markdown(root: &Path, skill_id: &str) -> anyhow::Result<String> {
    let conn = open_ctox_sqlite(root)?;
    let mut bundle = conn.prepare(
        "SELECT skill_name, class, state, description, source_path, cluster, updated_at
           FROM ctox_skill_bundles WHERE skill_id = ?1",
    )?;
    let (name, class_name, state, description, source_path, cluster, updated_at): (
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
    ) = bundle.query_row([skill_id], |row| {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
        ))
    })?;
    let mut text = format!(
        "# {name}\n\n{description}\n\n- Klasse: `{class_name}`\n- Status: `{state}`\n- Cluster: `{cluster}`\n- Quelle: `{}`\n- Aktualisiert: `{updated_at}`\n",
        source_path.unwrap_or_else(|| "unbekannt".to_owned())
    );
    let mut files = conn.prepare(
        "SELECT relative_path, substr(CAST(content_blob AS TEXT), 1, 120000) AS content_text
           FROM ctox_skill_files
          WHERE skill_id = ?1
          ORDER BY CASE WHEN relative_path = 'SKILL.md' THEN 0 ELSE 1 END, relative_path
          LIMIT 24",
    )?;
    let mut rows = files.query([skill_id])?;
    while let Some(row) = rows.next()? {
        let relative_path: String = row.get(0)?;
        let content: String = row.get(1)?;
        text.push_str(&format!("\n\n## {relative_path}\n\n{content}"));
    }
    Ok(text)
}

fn skillbook_markdown(root: &Path, skillbook_id: &str) -> anyhow::Result<String> {
    let conn = open_ctox_sqlite(root)?;
    let mut stmt = conn.prepare(
        "SELECT title, version, status, summary, mission, runtime_policy, answer_contract,
                workflow_backbone_json, routing_taxonomy_json, linked_runbooks_json, updated_at
           FROM knowledge_skillbooks WHERE skillbook_id = ?1",
    )?;
    let row = stmt.query_row([skillbook_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
        ))
    })?;
    Ok(format!(
        "# {}\n\n{}\n\n- Version: `{}`\n- Status: `{}`\n- Aktualisiert: `{}`\n\n## Mission\n\n{}\n\n## Runtime Policy\n\n{}\n\n## Answer Contract\n\n{}\n\n## Workflow Backbone\n\n```json\n{}\n```\n\n## Routing Taxonomy\n\n```json\n{}\n```\n\n## Linked Runbooks\n\n```json\n{}\n```",
        row.0, row.3, row.1, row.2, row.10, row.4, row.5, row.6, row.7, row.8, row.9
    ))
}

fn runbook_markdown(root: &Path, runbook_id: &str) -> anyhow::Result<String> {
    let conn = open_ctox_sqlite(root)?;
    let mut stmt = conn.prepare(
        "SELECT title, version, status, summary, problem_domain, item_labels_json, updated_at
           FROM knowledge_runbooks WHERE runbook_id = ?1",
    )?;
    let row = stmt.query_row([runbook_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut text = format!(
        "# {}\n\n{}\n\n- Version: `{}`\n- Status: `{}`\n- Domain: `{}`\n- Labels: `{}`\n- Aktualisiert: `{}`\n",
        row.0, row.3, row.1, row.2, row.4, row.5, row.6
    );
    let mut items = conn.prepare(
        "SELECT label, title, problem_class, chunk_text, structured_json, status, version
           FROM knowledge_runbook_items
          WHERE runbook_id = ?1
          ORDER BY label, updated_at DESC
          LIMIT 120",
    )?;
    let mut rows = items.query([runbook_id])?;
    while let Some(item) = rows.next()? {
        let label: String = item.get(0)?;
        let title: String = item.get(1)?;
        let problem_class: String = item.get(2)?;
        let chunk_text: String = item.get(3)?;
        let structured_json: String = item.get(4)?;
        let status: String = item.get(5)?;
        let version: String = item.get(6)?;
        text.push_str(&format!(
            "\n\n## {label} · {title}\n\n- Problemklasse: `{problem_class}`\n- Status: `{status}`\n- Version: `{version}`\n\n{chunk_text}\n\n```json\n{structured_json}\n```"
        ));
    }
    Ok(text)
}

#[derive(Debug, Clone)]
struct ParquetTableRef {
    title: String,
    description: String,
    path: PathBuf,
    row_count: Option<i64>,
    bytes: Option<i64>,
}

fn knowledge_dataframe_schema_payload(root: &Path, id: &str) -> anyhow::Result<Value> {
    let table = resolve_parquet_table(root, id)?;
    let mut lf = scan_parquet(&table.path)?;
    let schema = lf.collect_schema().context("collect parquet schema")?;
    let columns: Vec<Value> = schema
        .iter()
        .map(|(name, dtype)| {
            serde_json::json!({
                "name": name.to_string(),
                "dtype": format!("{dtype:?}")
            })
        })
        .collect();
    Ok(serde_json::json!({
        "ok": true,
        "id": id,
        "title": table.title,
        "columns": columns,
        "row_count": table.row_count,
        "bytes": table.bytes
    }))
}

fn knowledge_dataframe_rows_payload(
    root: &Path,
    id: &str,
    offset: usize,
    limit: usize,
) -> anyhow::Result<Value> {
    let table = resolve_parquet_table(root, id)?;
    let df = scan_parquet(&table.path)?
        .slice(offset as i64, limit as IdxSize)
        .collect()
        .context("collect parquet row window")?;
    let rows = dataframe_to_json_rows(&df)?;
    Ok(serde_json::json!({
        "ok": true,
        "id": id,
        "offset": offset,
        "limit": limit,
        "returned": rows.len(),
        "row_count": table.row_count,
        "rows": rows
    }))
}

fn resolve_parquet_table(root: &Path, id: &str) -> anyhow::Result<ParquetTableRef> {
    if let Some(table_id) = id.strip_prefix("table:") {
        let conn = open_ctox_sqlite(root)?;
        let mut stmt = conn.prepare(
            "SELECT title, description, parquet_path, row_count, bytes
               FROM knowledge_data_tables
              WHERE table_id = ?1 AND archived_at IS NULL",
        )?;
        let table = stmt.query_row([table_id], |row| {
            Ok(ParquetTableRef {
                title: row.get(0)?,
                description: row.get(1)?,
                path: PathBuf::from(row.get::<_, String>(2)?),
                row_count: Some(row.get(3)?),
                bytes: Some(row.get(4)?),
            })
        })?;
        if !table.path.is_file() {
            anyhow::bail!("parquet file is missing: {}", table.path.display());
        }
        return Ok(table);
    }
    for table in scan_runtime_parquet_table_refs(root)? {
        if format!("parquet:{}", short_path_hash(&table.path)) == id {
            return Ok(table);
        }
    }
    anyhow::bail!("unknown knowledge dataframe id: {id}")
}

fn scan_runtime_parquet_tables(root: &Path) -> anyhow::Result<Vec<Value>> {
    Ok(scan_runtime_parquet_table_refs(root)?
        .into_iter()
        .map(|table| {
            serde_json::json!({
                "id": format!("parquet:{}", short_path_hash(&table.path)),
                "kind": "dataframe",
                "title": table.title,
                "subtitle": "Runtime Parquet · Polars",
                "description": table.description,
                "parquet_path": table.path.display().to_string(),
                "row_count": table.row_count,
                "bytes": table.bytes,
                "updated_at": file_modified_label(&table.path)
            })
        })
        .collect())
}

fn scan_runtime_parquet_table_refs(root: &Path) -> anyhow::Result<Vec<ParquetTableRef>> {
    let mut out = Vec::new();
    let base = root.join("runtime").join("knowledge").join("data");
    collect_parquet_files(&base, &mut out)?;
    out.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.path.cmp(&b.path)));
    Ok(out)
}

fn collect_parquet_files(dir: &Path, out: &mut Vec<ParquetTableRef>) -> anyhow::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_parquet_files(&path, out)?;
        } else if file_type.is_file()
            && path.extension().and_then(|ext| ext.to_str()) == Some("parquet")
        {
            let bytes = fs::metadata(&path).map(|meta| meta.len() as i64).ok();
            let title = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("DataFrame")
                .replace(['_', '-'], " ");
            out.push(ParquetTableRef {
                title,
                description: "Record-shaped Knowledge aus runtime/knowledge/data.".to_owned(),
                path,
                row_count: None,
                bytes,
            });
        }
    }
    Ok(())
}

fn dataframe_to_json_rows(df: &DataFrame) -> anyhow::Result<Vec<Value>> {
    if df.height() == 0 {
        return Ok(Vec::new());
    }
    let mut buf = Vec::new();
    JsonWriter::new(&mut buf)
        .with_json_format(JsonFormat::JsonLines)
        .finish(&mut df.clone())
        .context("serialize DataFrame as JSON lines")?;
    let mut rows = Vec::new();
    for line in buf.split(|byte| *byte == b'\n') {
        if !line.is_empty() {
            rows.push(serde_json::from_slice(line)?);
        }
    }
    Ok(rows)
}

fn scan_parquet(path: &Path) -> PolarsResult<LazyFrame> {
    let pl_path = PlPath::new(&path.to_string_lossy());
    LazyFrame::scan_parquet(pl_path, ScanArgsParquet::default())
}

fn open_ctox_sqlite(root: &Path) -> anyhow::Result<Connection> {
    let path = ctox_sqlite_path(root);
    Connection::open(&path).with_context(|| format!("failed to open {}", path.display()))
}

fn ctox_sqlite_path(root: &Path) -> PathBuf {
    root.join("runtime").join("ctox.sqlite3")
}

fn parse_query(url: &str) -> HashMap<String, String> {
    let query = url.split_once('?').map(|(_, query)| query).unwrap_or("");
    url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

fn parse_usize_query(query: &HashMap<String, String>, key: &str, fallback: usize) -> usize {
    query
        .get(key)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(fallback)
}

fn short_path_hash(path: &Path) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(path.display().to_string().as_bytes());
    format!("{:x}", hasher.finalize())[..16].to_owned()
}

fn file_modified_label(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_default()
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
    response.add_header(Header::from_bytes("Cache-Control", "no-store").unwrap());
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
