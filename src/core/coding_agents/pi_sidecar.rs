//! P2: native owner of the pi-code coding sidecar
//! (`src/core/coding_agents/pi-sidecar`). Spawns the LocalTransport daemon and
//! drives one bounded turn over private platform IPC, then reaps it.
//!
//! This is the transport client the higher-level owner uses: it projects a
//! module's app source into a `CtoxTurnRequest.files` snapshot, runs one bounded
//! turn, and reads back the `CtoxTurnResponse` snapshot to record as P0 commits.
//! The sidecar is a bounded leaf executor — a fresh daemon per turn, killed on
//! drop; it never shares the daemon's process authority with the CTOX daemon.
use anyhow::Context;
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::execution::models::local_transport::{LocalStream, LocalTransport};

#[path = "anthropic_coding_bridge.rs"]
pub(crate) mod anthropic_coding_bridge;

use anthropic_coding_bridge::{AnthropicCodingBridge, BRIDGE_TOKEN_HEADER};

struct PreparedCodingTurnModel {
    model: Value,
    coding_plan_bridge: Option<AnthropicCodingBridge>,
    provider: String,
    model_id: String,
    account_id: Option<String>,
}

/// Path to the built sidecar bundle relative to the repo root (dev / tests).
pub fn sidecar_dist_path(repo_root: &Path) -> PathBuf {
    repo_root.join("src/core/coding_agents/pi-sidecar/dist/ctox-pi-sidecar.mjs")
}

/// The pi-sidecar bundle is embedded into the ctox binary at build time so a
/// deployed CTOX ships as one artifact (no source tree). Build order: the
/// sidecar bundle (`npm run build` in pi-sidecar) must exist before `cargo
/// build`; a CI/build step guarantees this.
const SIDECAR_BUNDLE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/core/coding_agents/pi-sidecar/dist/ctox-pi-sidecar.mjs"
));

/// Resolve a runnable sidecar bundle path for `root`, extracting the embedded
/// bytes to `<root>/coding-agents/ctox-pi-sidecar.mjs` when missing or
/// size-mismatched. An explicit `CTOX_PI_SIDECAR_DIST` override wins (dev /
/// custom deployments). This is the runtime resolver the owner uses.
pub fn resolve_sidecar_dist(root: &Path) -> anyhow::Result<PathBuf> {
    if let Ok(override_path) = std::env::var("CTOX_PI_SIDECAR_DIST") {
        let path = PathBuf::from(override_path);
        anyhow::ensure!(
            path.exists(),
            "CTOX_PI_SIDECAR_DIST does not exist: {}",
            path.display()
        );
        return Ok(path);
    }
    let dir = root.join("coding-agents");
    std::fs::create_dir_all(&dir).context("create sidecar runtime dir")?;
    let path = dir.join("ctox-pi-sidecar.mjs");
    let needs_write = match std::fs::metadata(&path) {
        Ok(meta) => meta.len() != SIDECAR_BUNDLE.len() as u64,
        Err(_) => true,
    };
    if needs_write {
        std::fs::write(&path, SIDECAR_BUNDLE).context("extract embedded sidecar bundle")?;
    }
    Ok(path)
}

/// The Business OS app skill: the system prompt that teaches the coding agent
/// how Business OS app modules are structured (module.json, `mount(ctx)`, the
/// shared kit, RxDB/WebRTC data boundary, command dispatch). This is
/// CTOX-specific knowledge, so it lives in the owner and is injected per turn —
/// the sidecar port stays a generic pi engine.
const BUSINESS_OS_APP_SKILL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/core/coding_agents/business-os-app-skill.md"
));

/// The coding agent's system prompt for a Business OS module turn: the app skill
/// plus the minimal tool-usage footer (the workspace is an in-memory projection
/// of the module's synced source, edited through the pi tools — no host FS).
pub fn business_os_system_prompt() -> String {
    format!(
        "{skill}\n\n## Tools and workspace\n\nUse the pi coding tools (read, \
edit, write, grep, find, ls) to inspect and change files. The filesystem is an \
isolated in-memory projection of this module's synced source — not the host \
filesystem. Make changes through write/edit; your edits are applied back as \
versioned commits to the module source.",
        skill = BUSINESS_OS_APP_SKILL
    )
}

/// Public descriptor for the inherited main route. Its private endpoint and
/// capability are resolved by the native owner immediately before a real turn.
pub fn gateway_model(root: &Path) -> Value {
    let gateway = crate::execution::responses::gateway::GatewayConfig::resolve_with_root(root);
    let model_id = gateway
        .active_model
        .unwrap_or_else(|| "ctox-gateway".to_string());
    serde_json::json!({
        "id": model_id,
        "name": "CTOX Model Gateway",
        "api": "openai-responses",
        "provider": "ctox-gateway",
        "baseUrl": "http://127.0.0.1:1/v1",
        "ctoxRoute": { "kind": "inherit_ctox" },
        "reasoning": false,
        "input": ["text"],
        "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 },
        "contextWindow": 0,
        "maxTokens": 0
    })
}

/// The coding default inherits CTOX's active main model through its native owner.
/// Provider credentials and routing remain server-side; callers may still
/// supply an explicit typed pi-ai model override.
pub fn coding_default_model(root: &Path) -> Value {
    gateway_model(root)
}

/// Server-authoritative public capability document. The browser receives no
/// credentials and may select the subscription route only after the owning
/// same-root listener is actually ready.
pub fn coding_model_capabilities(root: &Path) -> Value {
    let model = gateway_model(root);
    let active_model = model
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("ctox-gateway");
    let mut presets = vec![serde_json::json!({
        "id": "ctox",
        "label": format!("CTOX (Standard · {active_model})"),
        "default": true,
        "model": Value::Null,
    })];
    for route in crate::execution::cliproxyapi_host::instance_proxy_route_capabilities(root) {
        let provider = route.provider;
        let route_model = route.model;
        presets.push(serde_json::json!({
            "id": format!("{provider}-subscription-{route_model}"),
            "label": format!("{provider} subscription · {route_model}"),
            "default": false,
            "model": {
                "id": route_model,
                "name": format!("{route_model} via {provider} subscription"),
                "api": "openai-responses",
                "provider": "ctox-gateway",
                "baseUrl": crate::execution::cliproxyapi_host::instance_codex_proxy_base_url(),
                "headers": { "X-CTOX-Provider": provider },
                "reasoning": false,
                "input": ["text"],
                "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 },
                "contextWindow": 0,
                "maxTokens": 0
            }
        }));
    }
    // MiniMax Coding Plan is an independent direct account route. It does not
    // inherit or mutate CTOX's main provider and is never represented as
    // `ctox_proxy`. Only accounts whose encrypted secret handle resolves are
    // advertised.
    for account in
        crate::execution::models::minimax_coding::ready_accounts(root).unwrap_or_default()
    {
        for route_model in account.effective_models() {
            presets.push(serde_json::json!({
                // Length-prefixing makes the opaque id injective without
                // parsing provider/model identity back out of user input.
                "id": format!("minimax-coding-{}-{}-{route_model}", account.id.len(), account.id),
                "label": format!("MiniMax Coding Plan · {} · {route_model}", account.id),
                "default": false,
                "model": {
                    "id": route_model,
                    "name": format!("{route_model} via MiniMax Coding Plan ({})", account.id),
                    "api": "anthropic-messages",
                    "provider": "ctox-minimax-coding",
                    // Replaced with a fresh turn-scoped loopback bridge after
                    // server-authoritative preset resolution.
                    "baseUrl": "http://127.0.0.1:1",
                    "reasoning": true,
                    "input": ["text"],
                    "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 },
                    "contextWindow": 204800,
                    "maxTokens": 8192,
                    "ctoxRoute": {
                        "kind": "minimax_coding_plan",
                        "accountId": account.id,
                    }
                }
            }));
        }
    }
    // Kimi Coding Plan is likewise account-scoped and independent from both
    // the Kimi subscription route and CTOX's main model.
    for account in crate::execution::models::kimi_coding::ready_accounts(root).unwrap_or_default() {
        for route_model in account.effective_models() {
            presets.push(serde_json::json!({
                "id": format!("kimi-coding-{}-{}-{route_model}", account.id.len(), account.id),
                "label": format!("Kimi Coding Plan · {} · {route_model}", account.id),
                "default": false,
                "model": {
                    "id": route_model,
                    "name": format!("{route_model} via Kimi Coding Plan ({})", account.id),
                    "api": "anthropic-messages",
                    "provider": "ctox-kimi-coding",
                    "baseUrl": "http://127.0.0.1:1",
                    "reasoning": true,
                    "input": ["text"],
                    "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 },
                    "contextWindow": 1048576,
                    "maxTokens": 8192,
                    "ctoxRoute": {
                        "kind": "kimi_coding_plan",
                        "accountId": account.id,
                    }
                }
            }));
        }
    }
    serde_json::json!({
        "schema": "ctox.coding.models.v1",
        "default": {
            "mode": "inherit_ctox",
            "provider": "ctox-gateway",
            "model": active_model,
        },
        "presets": presets,
    })
}

/// Resolve one server-authored preset immediately before a turn. Business OS
/// sends only the opaque preset ID; provider URLs, routing headers and model
/// objects are never accepted from the browser command payload.
pub fn resolve_coding_model_preset(root: &Path, preset_id: &str) -> anyhow::Result<Option<Value>> {
    let preset_id = preset_id.trim();
    anyhow::ensure!(!preset_id.is_empty(), "coding model preset_id is required");
    let capabilities = coding_model_capabilities(root);
    let presets = capabilities
        .get("presets")
        .and_then(Value::as_array)
        .context("coding model capabilities are malformed")?;
    let mut matching = presets
        .iter()
        .filter(|preset| preset.get("id").and_then(Value::as_str) == Some(preset_id));
    let preset = matching
        .next()
        .with_context(|| format!("coding model preset is unavailable: {preset_id}"))?;
    anyhow::ensure!(
        matching.next().is_none(),
        "coding model preset is ambiguous"
    );
    match preset.get("model") {
        None | Some(Value::Null) => Ok(None),
        Some(model @ Value::Object(_)) => Ok(Some(model.clone())),
        Some(_) => anyhow::bail!("coding model preset is malformed"),
    }
}

/// A spawned sidecar daemon listening on private platform IPC. Killed + cleaned
/// on drop so a turn can never leak a live agent process.
struct SidecarDaemon {
    child: Child,
    unix_socket_path: Option<PathBuf>,
}

impl Drop for SidecarDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(path) = &self.unix_socket_path {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn spawn_sidecar(
    dist: &Path,
    transport: &LocalTransport,
    faux: bool,
) -> anyhow::Result<SidecarDaemon> {
    anyhow::ensure!(
        dist.exists(),
        "pi-sidecar bundle is not built: {} (run `npm run build` in pi-sidecar)",
        dist.display()
    );
    let mut command = Command::new("node");
    command
        .arg(dist)
        .arg(transport.endpoint_string())
        // Sandbox invariant: the sidecar is a bounded leaf executor whose rights
        // must be a strict SUBSET of the CTOX daemon's. It must NOT inherit the
        // daemon's environment (secret store, tokens, state-root paths). Start
        // from an empty env and grant only PATH (needed to resolve `node`) plus
        // the flags the turn needs; a real turn adds ONLY the gateway auth here.
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if faux {
        command.env("CTOX_PI_SIDECAR_FAUX", "1");
    }
    let child = command
        .spawn()
        .context("spawn pi-sidecar daemon (is `node` on PATH?)")?;
    Ok(SidecarDaemon {
        child,
        unix_socket_path: transport.unix_socket_path().map(Path::to_path_buf),
    })
}

fn connect_with_retry(
    transport: &LocalTransport,
    timeout: Duration,
) -> anyhow::Result<LocalStream> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let attempt_timeout = remaining.min(Duration::from_millis(100));
        match transport.connect_blocking(attempt_timeout) {
            Ok(stream) => return Ok(stream),
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                return Err(error).context("connect to pi-sidecar socket");
            }
        }
    }
}

const MAX_PI_TURN_REQUEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_PI_TURN_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const PI_TURN_TIMEOUT: Duration = Duration::from_secs(600);

fn read_line(stream: &mut LocalStream, max_bytes: usize) -> anyhow::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let read = stream.read(&mut byte).context("read turn response")?;
        if read == 0 || byte[0] == b'\n' {
            break;
        }
        buffer.push(byte[0]);
        anyhow::ensure!(
            buffer.len() <= max_bytes,
            "sidecar turn response is too large"
        );
    }
    Ok(buffer)
}

/// Run one bounded turn through a freshly spawned sidecar daemon: send `request`
/// (a `CtoxTurnRequest` JSON), return the `CtoxTurnResponse` JSON. `faux` runs
/// the sidecar's offline no-model mode (owner integration tests).
pub fn run_pi_turn(dist: &Path, request: &Value, faux: bool) -> anyhow::Result<Value> {
    let endpoint_id = Uuid::new_v4();
    let transport = LocalTransport::ipc_for_host(
        std::env::temp_dir().join(format!("ctox-pi-{endpoint_id}.sock")),
        format!("ctox-pi-{endpoint_id}"),
    );
    let _daemon = spawn_sidecar(dist, &transport, faux)?;
    let mut stream = connect_with_retry(&transport, Duration::from_secs(10))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .context("set pi-sidecar write timeout")?;
    stream
        .set_read_timeout(Some(PI_TURN_TIMEOUT))
        .context("set pi-sidecar turn timeout")?;

    let mut line = serde_json::to_string(request).context("serialize turn request")?;
    anyhow::ensure!(
        line.len() < MAX_PI_TURN_REQUEST_BYTES,
        "sidecar turn request is too large"
    );
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .context("write turn request")?;
    stream.flush().ok();

    let response_bytes = read_line(&mut stream, MAX_PI_TURN_RESPONSE_BYTES)?;
    anyhow::ensure!(
        !response_bytes.is_empty(),
        "sidecar closed without a response"
    );
    let response: Value =
        serde_json::from_slice(&response_bytes).context("parse turn response JSON")?;
    Ok(response)
}

/// Project a module's synced app source (`business_module_source_files` records)
/// into a `{path -> content}` map for a `CtoxTurnRequest.files` snapshot. This is
/// the app-source-projection workspace model: the sidecar edits a materialized
/// view of the source records; its writes come back as P0 commits. No host FS.
pub fn project_module_source(
    root: &Path,
    module_id: &str,
) -> anyhow::Result<serde_json::Map<String, Value>> {
    let records = crate::business_os::store::pull_collection_records(
        root,
        "business_module_source_files",
        None,
        None,
    )?;
    let mut files = serde_json::Map::new();
    if let Some(documents) = records.get("documents").and_then(Value::as_array) {
        for document in documents {
            if document.get("module_id").and_then(Value::as_str) != Some(module_id) {
                continue;
            }
            if document.get("_deleted").and_then(Value::as_bool) == Some(true) {
                continue;
            }
            let (Some(path), Some(content)) = (
                document.get("path").and_then(Value::as_str),
                document.get("content").and_then(Value::as_str),
            ) else {
                continue;
            };
            files.insert(path.to_string(), Value::String(content.to_string()));
        }
    }
    Ok(files)
}

/// Apply a turn's returned snapshot back into the module's app source. Each file
/// is written through the same policy-gated source path that records P0
/// versions/commits — the agent proposed, the trusted owner disposes. The
/// sidecar env cwd prefix (`/workspace/`) is stripped to the module-relative
/// path. Returns the paths written.
pub fn apply_turn_snapshot(
    root: &Path,
    module_id: &str,
    snapshot: &[Value],
) -> anyhow::Result<Vec<String>> {
    let mut applied = Vec::new();
    for entry in snapshot {
        if entry.get("kind").and_then(Value::as_str) != Some("file") {
            continue;
        }
        let Some(raw_path) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        let path = raw_path
            .strip_prefix("/workspace/")
            .unwrap_or_else(|| raw_path.trim_start_matches('/'));
        let Some(content) = entry.get("content").and_then(Value::as_str) else {
            continue;
        };
        crate::business_os::store::save_module_source_record(
            root,
            crate::business_os::store::ModuleSourceSaveMutation {
                module_id: module_id.to_string(),
                path: path.to_string(),
                content: content.to_string(),
            },
        )?;
        applied.push(path.to_string());
    }
    Ok(applied)
}

/// The owner's core delegation primitive: one bounded coding turn against a
/// module's app source. Project the source into the request, run the pi turn
/// through the sidecar (`faux` = offline no-model), then apply the resulting
/// snapshot back into the source (recording P0 versions). Returns a summary.
pub fn run_module_coding_turn(
    root: &Path,
    dist: &Path,
    module_id: &str,
    prompt: &str,
    faux: bool,
    model_override: Option<Value>,
) -> anyhow::Result<Value> {
    run_module_coding_turn_inner(root, dist, module_id, prompt, faux, model_override, None)
}

fn prepare_coding_turn_model(
    root: &Path,
    model_override: Option<Value>,
    coding_plan_upstream_override: Option<&str>,
    require_unique_subscription_account: bool,
) -> anyhow::Result<PreparedCodingTurnModel> {
    let mut model = model_override.unwrap_or_else(|| coding_default_model(root));
    if model.pointer("/ctoxRoute/kind").and_then(Value::as_str) == Some("inherit_ctox") {
        return prepare_inherited_coding_model(root);
    }
    let model_id = model
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("coding route is missing model id")?
        .to_owned();
    let route_kind = model
        .pointer("/ctoxRoute/kind")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut coding_plan_bridge = None;
    let (provider, account_id) = if matches!(
        route_kind.as_deref(),
        Some("minimax_coding_plan" | "kimi_coding_plan")
    ) {
        let account_id = model
            .pointer("/ctoxRoute/accountId")
            .and_then(Value::as_str)
            .context("coding-plan route is missing accountId")?
            .to_owned();
        let (provider, api_key, configured_upstream_base_url) = match route_kind.as_deref() {
            Some("minimax_coding_plan") => {
                anyhow::ensure!(
                    model.get("provider").and_then(Value::as_str) == Some("ctox-minimax-coding")
                        && model.get("api").and_then(Value::as_str) == Some("anthropic-messages"),
                    "MiniMax coding route is malformed"
                );
                let account = crate::execution::models::minimax_coding::resolve_ready_account(
                    root,
                    &account_id,
                )?;
                anyhow::ensure!(
                    account
                        .effective_models()
                        .iter()
                        .any(|allowed| allowed == &model_id),
                    "MiniMax coding model is not allowed for account {account_id}"
                );
                (
                    "minimax_coding_plan".to_owned(),
                    crate::execution::models::minimax_coding::read_api_key(root, &account)?,
                    account.endpoint_profile.base_url(),
                )
            }
            Some("kimi_coding_plan") => {
                anyhow::ensure!(
                    model.get("provider").and_then(Value::as_str) == Some("ctox-kimi-coding")
                        && model.get("api").and_then(Value::as_str) == Some("anthropic-messages"),
                    "Kimi coding route is malformed"
                );
                let account = crate::execution::models::kimi_coding::resolve_ready_account(
                    root,
                    &account_id,
                )?;
                anyhow::ensure!(
                    account
                        .effective_models()
                        .iter()
                        .any(|allowed| allowed == &model_id),
                    "Kimi coding model is not allowed for account {account_id}"
                );
                (
                    "kimi_coding_plan".to_owned(),
                    crate::execution::models::kimi_coding::read_api_key(root, &account)?,
                    account.endpoint_profile.base_url(),
                )
            }
            _ => unreachable!("route kind was checked above"),
        };
        let upstream_base_url =
            coding_plan_upstream_override.unwrap_or(configured_upstream_base_url);
        let bridge = AnthropicCodingBridge::spawn(api_key, upstream_base_url)?;
        model["baseUrl"] = Value::String(bridge.base_url().to_owned());
        model["headers"] = serde_json::json!({
            (BRIDGE_TOKEN_HEADER): bridge.capability_token(),
        });
        // The account selector has already been consumed by the native owner.
        // Do not project it into the less-privileged sidecar request.
        if let Some(object) = model.as_object_mut() {
            object.remove("ctoxRoute");
        }
        coding_plan_bridge = Some(bridge);
        (provider, Some(account_id))
    } else if let Some(provider) = model
        .pointer("/headers/X-CTOX-Provider")
        .and_then(Value::as_str)
    {
        let provider = provider.trim().to_ascii_lowercase();
        let account_id = require_unique_subscription_account
            .then(|| {
                crate::execution::cliproxyapi_host::unique_instance_proxy_account_for_route(
                    root, &provider, &model_id,
                )
            })
            .transpose()?;
        (provider, account_id)
    } else {
        (
            model
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or("ctox-gateway")
                .to_owned(),
            None,
        )
    };

    Ok(PreparedCodingTurnModel {
        model,
        coding_plan_bridge,
        provider,
        model_id,
        account_id,
    })
}

struct InheritedCodingRoute {
    provider: String,
    model_id: String,
    base_url: String,
    credential_key: &'static str,
    api: &'static str,
}

fn resolve_inherited_coding_route(root: &Path) -> anyhow::Result<InheritedCodingRoute> {
    use crate::execution::models::{runtime_env, runtime_kernel, runtime_state};

    let runtime = runtime_kernel::InferenceRuntimeKernel::resolve(root)?;
    let mut settings = runtime_env::load_persisted_runtime_env_map_cached(root)?;
    runtime_state::apply_runtime_state_to_env_map(&mut settings, &runtime.state);
    let provider = runtime_state::infer_api_provider_from_env_map(&settings);
    // The main spec owns provider/model, endpoint and credential selection.
    // Pi's existing wire adapters handle the actual provider edge: direct
    // MiniMax/OpenRouter speak Chat Completions, not the proxy's Responses API.
    anyhow::ensure!(
        matches!(
            provider.as_str(),
            "openai" | "ctox_proxy" | "azure_foundry" | "minimax" | "openrouter"
        ),
        "inherited Pi route does not support main provider {provider}; select a supported coding preset"
    );
    let model_id = runtime
        .state
        .active_or_selected_model()
        .filter(|model| !model.trim().is_empty())
        .context("CTOX main route has no selected model")?
        .to_owned();
    let spec = crate::execution::agent::turn_loop::resolve_api_model_provider_spec(
        &model_id,
        &settings,
        Some(&runtime),
    );
    let (base_url, credential_key) = if provider == "openai" {
        (runtime.internal_responses_base_url(), "OPENAI_API_KEY")
    } else {
        let spec = spec.context("CTOX main provider/model route is not available to Pi")?;
        anyhow::ensure!(
            spec.wire_api == "responses" && spec.subscription_provider.is_none(),
            "CTOX main provider needs a different coding protocol adapter"
        );
        (spec.base_url, spec.env_key)
    };
    let endpoint = url::Url::parse(&base_url).context("CTOX main route has an invalid endpoint")?;
    anyhow::ensure!(
        matches!(endpoint.scheme(), "http" | "https")
            && endpoint.host_str().is_some()
            && endpoint.username().is_empty()
            && endpoint.password().is_none()
            && endpoint.port() != Some(12434),
        "CTOX main route has an unsupported endpoint"
    );
    let api = match provider.as_str() {
        "minimax" | "openrouter" => "openai-completions",
        _ => "openai-responses",
    };
    Ok(InheritedCodingRoute {
        provider,
        model_id,
        base_url,
        credential_key,
        api,
    })
}

/// Operator-only nonsecret route evidence. No sidecar/listener/network call or
/// Business OS data access; origin omits paths, queries and credentials.
pub fn inherited_coding_route_status(root: &Path) -> anyhow::Result<Value> {
    let route = resolve_inherited_coding_route(root)?;
    Ok(serde_json::json!({
        "ok": true,
        "schema": "ctox.coding.main-route.v1",
        "provider": route.provider,
        "model": route.model_id,
        "upstream_origin": url::Url::parse(&route.base_url)?.origin().ascii_serialization(),
        "wire_api": route.api,
    }))
}

fn prepare_inherited_coding_model(root: &Path) -> anyhow::Result<PreparedCodingTurnModel> {
    let route = resolve_inherited_coding_route(root)?;
    prepare_inherited_coding_model_route(root, route)
}

fn prepare_inherited_coding_model_route(
    root: &Path,
    route: InheritedCodingRoute,
) -> anyhow::Result<PreparedCodingTurnModel> {
    let settings = crate::execution::models::runtime_env::load_runtime_env_map(root)?;
    let api_key = settings
        .get(route.credential_key)
        .filter(|value| !value.trim().is_empty())
        .with_context(|| {
            format!(
                "CTOX main provider {} has no configured credential",
                route.provider
            )
        })?
        .to_owned();
    let bridge = if route.api == "openai-completions" {
        AnthropicCodingBridge::spawn_chat_completions(api_key, &route.base_url, &route.model_id)?
    } else {
        AnthropicCodingBridge::spawn_responses(api_key, &route.base_url, &route.model_id)?
    };
    let mut model = gateway_model(root);
    model["id"] = Value::String(route.model_id.clone());
    model["api"] = Value::String(route.api.to_owned());
    if route.api == "openai-completions" {
        // Explicit compatibility survives the loopback endpoint/provider alias;
        // SDK hostname heuristics cannot recognize the private bridge.
        model["compat"] = serde_json::json!({
            "supportsDeveloperRole": false,
            "supportsStore": false,
            "supportsReasoningEffort": false,
            "supportsStrictMode": false,
            "maxTokensField": "max_tokens",
        });
    }
    model["baseUrl"] = Value::String(format!("{}/v1", bridge.base_url()));
    model["headers"] = serde_json::json!({ (BRIDGE_TOKEN_HEADER): bridge.capability_token() });
    model
        .as_object_mut()
        .expect("native model descriptor")
        .remove("ctoxRoute");
    Ok(PreparedCodingTurnModel {
        model,
        coding_plan_bridge: Some(bridge),
        provider: route.provider,
        model_id: route.model_id,
        account_id: None,
    })
}

/// Execute an operator-requested, bounded live smoke without reading or
/// mutating Business OS source. The opaque preset is resolved immediately
/// before the turn; the returned evidence contains no URL, account identifier,
/// credential, bridge capability or model response text.
pub fn run_coding_preset_smoke(
    root: &Path,
    dist: &Path,
    preset_id: &str,
    prompt: Option<&str>,
) -> anyhow::Result<Value> {
    run_coding_preset_smoke_inner(root, dist, preset_id, prompt, None, None)
}

fn run_coding_preset_smoke_inner(
    root: &Path,
    dist: &Path,
    preset_id: &str,
    prompt: Option<&str>,
    coding_plan_upstream_override: Option<&str>,
    subscription_proxy_override: Option<&str>,
) -> anyhow::Result<Value> {
    let model = resolve_coding_model_preset(root, preset_id)?
        .context("the inherited CTOX preset is not an independent provider smoke target")?;
    let main_model_before = crate::inference::runtime_env::effective_chat_model(root);
    let mut prepared =
        prepare_coding_turn_model(root, Some(model), coding_plan_upstream_override, true)?;
    if let Some(proxy_base_url) = subscription_proxy_override {
        anyhow::ensure!(
            prepared
                .model
                .pointer("/headers/X-CTOX-Provider")
                .and_then(Value::as_str)
                .is_some(),
            "subscription proxy override requires a native subscription preset"
        );
        prepared.model["baseUrl"] = Value::String(proxy_base_url.to_owned());
    }
    let account_id = prepared
        .account_id
        .as_deref()
        .context("coding preset is not bound to a provider account")?;
    let account_digest = ring::digest::digest(&ring::digest::SHA256, account_id.as_bytes());
    let account_id_sha256 = account_digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let request = serde_json::json!({
        "id": "operator-provider-smoke",
        "prompt": prompt.unwrap_or(
            "Edit index.js and change only the exported value from 1 to 2. Use the edit tool."
        ),
        "files": { "index.js": "export const v = 1;\n" },
        "tools": ["read", "edit"],
        "maxAssistantTurns": 4,
        "systemPrompt": "This is a bounded provider-route smoke. Edit only index.js in the in-memory workspace.",
        "model": prepared.model,
    });
    let response = run_pi_turn(dist, &request, false)?;
    #[cfg(test)]
    drop(prepared.coding_plan_bridge);
    anyhow::ensure!(
        response.get("ok").and_then(Value::as_bool) == Some(true),
        "provider smoke Pi turn failed"
    );
    let edited = response
        .get("snapshot")
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries.iter().find(|entry| {
                entry.get("kind").and_then(Value::as_str) == Some("file")
                    && entry
                        .get("path")
                        .and_then(Value::as_str)
                        .is_some_and(|path| path.ends_with("/index.js"))
            })
        })
        .and_then(|entry| entry.get("content"))
        .and_then(Value::as_str)
        .context("provider smoke did not return index.js")?;
    let compact = edited
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    anyhow::ensure!(
        compact.contains("v=2"),
        "provider smoke did not apply the expected bounded edit"
    );
    let main_model_after = crate::inference::runtime_env::effective_chat_model(root);
    anyhow::ensure!(
        main_model_after == main_model_before,
        "provider smoke changed the CTOX main model"
    );
    Ok(serde_json::json!({
        "ok": true,
        "schema": "ctox.coding.provider-smoke.v1",
        "provider": prepared.provider,
        "model": prepared.model_id,
        "account_id_sha256": account_id_sha256,
        "main_model_unchanged": true,
        "bounded_edit_verified": true,
    }))
}

/// Test-only listener seam for proving that an opaque subscription preset
/// crosses the real Pi process and the native provider router. Production
/// always resolves the fixed instance-loopback listener.
#[cfg(test)]
pub(crate) fn run_coding_preset_smoke_with_subscription_proxy_for_test(
    root: &Path,
    dist: &Path,
    preset_id: &str,
    proxy_base_url: &str,
) -> anyhow::Result<Value> {
    crate::execution::cliproxyapi_host::mark_instance_codex_proxy_ready_for_test(root);
    run_coding_preset_smoke_inner(root, dist, preset_id, None, None, Some(proxy_base_url))
}

/// Internal owner seam. Production always leaves
/// `coding_plan_upstream_override` unset. Tests use a controlled loopback
/// upstream so the complete preset -> account -> bridge -> Pi route can be
/// proven without adding an ambient runtime toggle or contacting a provider.
fn run_module_coding_turn_inner(
    root: &Path,
    dist: &Path,
    module_id: &str,
    prompt: &str,
    faux: bool,
    model_override: Option<Value>,
    coding_plan_upstream_override: Option<&str>,
) -> anyhow::Result<Value> {
    let files = project_module_source(root, module_id)?;
    let mut request = serde_json::json!({
        "id": module_id,
        "prompt": prompt,
        "files": files.clone(),
        "maxAssistantTurns": 8,
        // The agent gets the Business OS app skill so it edits modules the way
        // the shell/kit/data-boundary contract requires (not as a generic web page).
        "systemPrompt": business_os_system_prompt(),
    });
    // Omission inherits CTOX's active provider/model through the main gateway;
    // an explicit server-authored model may choose another provider route.
    let mut coding_plan_bridge = None;
    if !faux {
        let prepared =
            prepare_coding_turn_model(root, model_override, coding_plan_upstream_override, false)?;
        request["model"] = prepared.model;
        coding_plan_bridge = prepared.coding_plan_bridge;
    }
    let response = run_pi_turn(dist, &request, faux)?;
    drop(coding_plan_bridge);
    anyhow::ensure!(
        response.get("ok").and_then(Value::as_bool) == Some(true),
        "pi-sidecar turn failed: {}",
        response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    );
    let empty = Vec::new();
    let snapshot = response
        .get("snapshot")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let applied = apply_changed_turn_snapshot(root, module_id, &files, snapshot)?;
    let message_count = response
        .get("messages")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    // Record the turn under the module's coding session (one session per app) so
    // the workbench can show a per-app history. Best-effort: a session-log hiccup
    // must not discard an edit that already landed in the source.
    if let Err(error) = crate::business_os::store::record_coding_agent_session_turn(
        root,
        module_id,
        prompt,
        &applied,
        message_count,
    ) {
        eprintln!("coding session log failed for {module_id}: {error}");
    }
    Ok(serde_json::json!({
        "ok": true,
        "module_id": module_id,
        "applied_files": applied,
        "message_count": message_count,
        "assistant_text": coding_turn_assistant_text(&response),
    }))
}

fn apply_changed_turn_snapshot(
    root: &Path,
    module_id: &str,
    baseline: &serde_json::Map<String, Value>,
    snapshot: &[Value],
) -> anyhow::Result<Vec<String>> {
    let mut changed = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in snapshot {
        if entry.get("kind").and_then(Value::as_str) != Some("file") {
            continue;
        }
        let (Some(raw_path), Some(content)) = (
            entry.get("path").and_then(Value::as_str),
            entry.get("content").and_then(Value::as_str),
        ) else {
            continue;
        };
        let path = raw_path
            .strip_prefix("/workspace/")
            .unwrap_or_else(|| raw_path.trim_start_matches('/'));
        anyhow::ensure!(seen.insert(path), "duplicate coding snapshot path: {path}");
        let before = baseline.get(path).and_then(Value::as_str);
        if before == Some(content) {
            continue;
        }
        // Validate the complete proposed change set before applying any file.
        // A stale source projection must not replace a newer on-disk release.
        crate::business_os::store::ensure_module_source_record_current(
            root, module_id, path, before,
        )?;
        changed.push((path, content, before));
    }
    let mut applied = Vec::new();
    for (path, content, before) in changed {
        crate::business_os::store::save_module_source_record_if_current(
            root,
            crate::business_os::store::ModuleSourceSaveMutation {
                module_id: module_id.to_string(),
                path: path.to_string(),
                content: content.to_string(),
            },
            before,
        )?;
        applied.push(path.to_string());
    }
    Ok(applied)
}

fn coding_turn_assistant_text(response: &Value) -> String {
    let mut text = Vec::new();
    for message in response
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        if let Some(content) = message.get("content").and_then(Value::as_str) {
            text.push(content);
        } else if let Some(content) = message.get("content").and_then(Value::as_array) {
            for block in content {
                if block.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(value) = block.get("text").and_then(Value::as_str) {
                        text.push(value);
                    }
                }
            }
        }
    }
    text.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tiny_http::{Header, Response, Server};

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn node_available() -> bool {
        Command::new("node")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn spawn_anthropic_edit_upstream(
        expected_model: &'static str,
        expected_api_key: &'static str,
    ) -> anyhow::Result<(String, std::thread::JoinHandle<()>)> {
        let server =
            Server::http("127.0.0.1:0").map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let address = server.server_addr().to_ip().context("fake upstream IP")?;
        let worker = std::thread::spawn(move || {
            for turn in 0..2 {
                let mut request = server
                    .recv_timeout(Duration::from_secs(30))
                    .expect("fake upstream receive")
                    .expect("Pi request reaches fake upstream");
                assert_eq!(request.url(), "/v1/messages");
                let api_key_header = request
                    .headers()
                    .iter()
                    .find(|header| {
                        header
                            .field
                            .as_str()
                            .as_str()
                            .eq_ignore_ascii_case("x-api-key")
                    })
                    .map(|header| header.value.as_str().to_owned());
                let bridge_header = request.headers().iter().find(|header| {
                    header
                        .field
                        .as_str()
                        .as_str()
                        .eq_ignore_ascii_case(BRIDGE_TOKEN_HEADER)
                });
                assert_eq!(api_key_header.as_deref(), Some(expected_api_key));
                assert!(bridge_header.is_none());
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                let body: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(body["model"], expected_model);
                let sse = if turn == 0 {
                    concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-smoke-1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"test\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tool-smoke-1\",\"name\":\"edit\",\"input\":{}}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\":\\\"index.js\\\",\\\"edits\\\":[{\\\"oldText\\\":\\\"v = 1\\\",\\\"newText\\\":\\\"v = 2\\\"}]}\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_delta\n",
                        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":8}}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                    )
                } else {
                    concat!(
                        "event: message_start\n",
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-smoke-2\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"test\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n",
                        "event: content_block_start\n",
                        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
                        "event: content_block_delta\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Done.\"}}\n\n",
                        "event: content_block_stop\n",
                        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        "event: message_delta\n",
                        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":2}}\n\n",
                        "event: message_stop\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                    )
                };
                request
                    .respond(Response::from_string(sse).with_header(
                        Header::from_bytes("content-type", "text/event-stream").unwrap(),
                    ))
                    .unwrap();
            }
        });
        Ok((format!("http://{address}"), worker))
    }

    fn seed_unrelated_main_model(root: &Path) -> anyhow::Result<()> {
        let mut env = BTreeMap::new();
        env.insert("CTOX_API_PROVIDER".to_owned(), "openai".to_owned());
        env.insert(
            "CTOX_CHAT_MODEL_BASE".to_owned(),
            "main-model-must-stay-selected".to_owned(),
        );
        env.insert(
            "CTOX_CHAT_MODEL".to_owned(),
            "main-model-must-stay-selected".to_owned(),
        );
        crate::inference::runtime_env::save_runtime_env_map(root, &env)
    }

    #[test]
    fn faux_sidecar_serves_a_turn_over_the_socket() -> anyhow::Result<()> {
        let dist = sidecar_dist_path(&repo_root());
        if !dist.exists() {
            eprintln!("SKIP: pi-sidecar bundle not built ({})", dist.display());
            return Ok(());
        }
        if !node_available() {
            eprintln!("SKIP: `node` not on PATH");
            return Ok(());
        }

        let request = serde_json::json!({
            "id": "rust-1",
            "prompt": "add a marker",
            "files": { "index.js": "export const v = 1;\n" },
            "maxAssistantTurns": 4
        });
        let response = run_pi_turn(&dist, &request, true)?;

        assert_eq!(response["ok"], Value::Bool(true), "turn ok");
        assert_eq!(response["id"], "rust-1", "response echoes id");
        let has_marker = response["snapshot"]
            .as_array()
            .map(|entries| {
                entries.iter().any(|entry| {
                    entry["path"]
                        .as_str()
                        .map(|path| path.ends_with("faux-marker.js"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        assert!(has_marker, "faux write should round-trip over the socket");
        Ok(())
    }

    #[test]
    fn projects_module_source_records_into_a_files_map() -> anyhow::Result<()> {
        use crate::business_os::store::{load_module_source_records, ModuleSourceLoadMutation};

        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let app_root = root.join("src").join("apps").join("business-os");
        std::fs::create_dir_all(app_root.join("modules").join("widget"))?;
        std::fs::write(app_root.join("index.html"), b"<!doctype html>")?;
        std::fs::write(
            app_root.join("modules").join("widget").join("module.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": "widget",
                "title": "Widget",
                "entry": "modules/widget/index.html"
            }))?,
        )?;
        std::fs::write(
            app_root.join("modules").join("widget").join("index.js"),
            "export const v = 1;\n",
        )?;

        load_module_source_records(
            root,
            &ModuleSourceLoadMutation {
                module_id: "widget".to_string(),
            },
        )?;

        let files = project_module_source(root, "widget")?;
        assert!(!files.is_empty(), "projected some source files");
        let has_content = files
            .values()
            .any(|value| value.as_str() == Some("export const v = 1;\n"));
        assert!(
            has_content,
            "widget source content projected into the files map"
        );
        Ok(())
    }

    #[test]
    fn run_module_coding_turn_records_the_faux_edit() -> anyhow::Result<()> {
        use crate::business_os::store::{load_module_source_records, ModuleSourceLoadMutation};

        let dist = sidecar_dist_path(&repo_root());
        if !dist.exists() {
            eprintln!("SKIP: pi-sidecar bundle not built ({})", dist.display());
            return Ok(());
        }
        if !node_available() {
            eprintln!("SKIP: `node` not on PATH");
            return Ok(());
        }

        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let app_root = root.join("src").join("apps").join("business-os");
        std::fs::create_dir_all(app_root.join("modules").join("widget"))?;
        std::fs::write(app_root.join("index.html"), b"<!doctype html>")?;
        std::fs::write(
            app_root.join("modules").join("widget").join("module.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": "widget",
                "title": "Widget",
                "entry": "modules/widget/index.html"
            }))?,
        )?;
        std::fs::write(
            app_root.join("modules").join("widget").join("index.js"),
            "export const v = 1;\n",
        )?;
        load_module_source_records(
            root,
            &ModuleSourceLoadMutation {
                module_id: "widget".to_string(),
            },
        )?;

        let summary = run_module_coding_turn(root, &dist, "widget", "add a marker", true, None)?;
        assert_eq!(summary["ok"], Value::Bool(true), "owner turn ok");

        // The faux edit must now be part of the module's source records — proving
        // the full owner loop project -> pi turn -> apply -> P0 source records.
        let files = project_module_source(root, "widget")?;
        assert!(
            files.keys().any(|path| path.ends_with("faux-marker.js")),
            "faux edit recorded into module source via the owner loop"
        );
        Ok(())
    }

    #[test]
    fn apply_snapshot_round_trips_a_seeded_file_edit() -> anyhow::Result<()> {
        use crate::business_os::store::{load_module_source_records, ModuleSourceLoadMutation};

        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let app_root = root.join("src").join("apps").join("business-os");
        std::fs::create_dir_all(app_root.join("modules").join("widget"))?;
        std::fs::write(app_root.join("index.html"), b"<!doctype html>")?;
        std::fs::write(
            app_root.join("modules").join("widget").join("module.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": "widget",
                "title": "Widget",
                "entry": "modules/widget/index.html"
            }))?,
        )?;
        std::fs::write(
            app_root.join("modules").join("widget").join("index.js"),
            "export const v = 1;\n",
        )?;
        load_module_source_records(
            root,
            &ModuleSourceLoadMutation {
                module_id: "widget".to_string(),
            },
        )?;

        // Learn the projected path key for index.js, then simulate a turn snapshot
        // that edited exactly that file (with the sidecar env cwd prefix).
        let before = project_module_source(root, "widget")?;
        let key = before
            .keys()
            .find(|path| path.ends_with("index.js"))
            .cloned()
            .expect("index.js is projected");
        let snapshot = vec![serde_json::json!({
            "path": format!("/workspace/{key}"),
            "kind": "file",
            "content": "export const v = 2;\n"
        })];
        apply_turn_snapshot(root, "widget", &snapshot)?;

        // The SAME path must now carry the edit — not a nested duplicate.
        let after = project_module_source(root, "widget")?;
        assert_eq!(
            after.get(&key).and_then(Value::as_str),
            Some("export const v = 2;\n"),
            "a real edit round-trips project -> apply to the same module path ({key})"
        );
        let next = vec![serde_json::json!({
            "path": format!("/workspace/{key}"),
            "kind": "file",
            "content": "export const v = 3;\n"
        })];
        let applied = apply_changed_turn_snapshot(root, "widget", &after, &next)?;
        assert_eq!(applied, vec![key.clone()]);
        assert_eq!(
            project_module_source(root, "widget")?[&key],
            "export const v = 3;\n"
        );
        Ok(())
    }

    #[test]
    fn unchanged_coding_snapshot_does_not_restore_stale_source() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let live = temp.path().join("live.js");
        std::fs::write(&live, "new release")?;
        let baseline = serde_json::json!({"live.js":"old projection"});
        let snapshot = vec![
            serde_json::json!({"kind":"file","path":"/workspace/live.js","content":"old projection"}),
        ];
        let applied = apply_changed_turn_snapshot(
            temp.path(),
            "widget",
            baseline.as_object().unwrap(),
            &snapshot,
        )?;
        assert!(
            applied.is_empty(),
            "a read-only turn must not write any source"
        );
        assert_eq!(std::fs::read_to_string(live)?, "new release");
        Ok(())
    }

    #[test]
    fn changed_coding_snapshot_rejects_stale_files_before_any_write() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let app = temp.path().join("src/apps/business-os");
        let module = app.join("modules/widget");
        std::fs::create_dir_all(&module)?;
        std::fs::write(app.join("index.html"), "<!doctype html>")?;
        std::fs::write(
            module.join("module.json"),
            r#"{"id":"widget","title":"Widget","entry":"modules/widget/index.html"}"#,
        )?;
        std::fs::write(module.join("index.js"), "before")?;
        std::fs::write(module.join("other.js"), "newer live version")?;
        let baseline = serde_json::json!({"index.js":"before", "other.js":"stale"});
        let snapshot = vec![
            serde_json::json!({"kind":"file","path":"/workspace/index.js","content":"proposed first edit"}),
            serde_json::json!({"kind":"file","path":"/workspace/other.js","content":"proposed stale edit"}),
        ];
        let result = apply_changed_turn_snapshot(
            temp.path(),
            "widget",
            baseline.as_object().unwrap(),
            &snapshot,
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("coding source conflict"));
        assert_eq!(std::fs::read_to_string(module.join("index.js"))?, "before");
        assert_eq!(
            std::fs::read_to_string(module.join("other.js"))?,
            "newer live version"
        );
        Ok(())
    }

    #[test]
    fn coding_turn_returns_only_assistant_text_not_reasoning_or_tools() {
        let response = serde_json::json!({"messages":[
            {"role":"user","content":"private prompt"},
            {"role":"toolResult","content":[{"type":"text","text":"tool output"}]},
            {"role":"assistant","content":[{"type":"thinking","thinking":"internal reasoning"},{"type":"text","text":"review result"}]},
            {"role":"assistant","content":"final result"}
        ]});
        assert_eq!(
            coding_turn_assistant_text(&response),
            "review result\nfinal result"
        );
    }

    #[test]
    fn embedded_sidecar_extracts_and_runs() -> anyhow::Result<()> {
        // The override must not leak in from the environment for this test.
        std::env::remove_var("CTOX_PI_SIDECAR_DIST");
        let temp = tempfile::tempdir()?;
        let root = temp.path();

        let dist = resolve_sidecar_dist(root)?;
        assert!(
            dist.exists(),
            "embedded sidecar extracted to {}",
            dist.display()
        );
        assert_eq!(
            std::fs::metadata(&dist)?.len(),
            SIDECAR_BUNDLE.len() as u64,
            "extracted bundle size matches the embedded bytes"
        );
        // Idempotent: a second resolve does not rewrite / returns the same path.
        assert_eq!(resolve_sidecar_dist(root)?, dist);

        if !node_available() {
            eprintln!("SKIP: `node` not on PATH (extraction verified, run skipped)");
            return Ok(());
        }
        // The extracted bundle must actually be runnable end-to-end.
        let request = serde_json::json!({
            "id": "embed-1",
            "prompt": "x",
            "files": { "index.js": "1\n" },
            "maxAssistantTurns": 4
        });
        let response = run_pi_turn(&dist, &request, true)?;
        assert_eq!(
            response["ok"],
            Value::Bool(true),
            "the extracted embedded sidecar serves a turn"
        );
        Ok(())
    }

    #[test]
    fn business_os_system_prompt_carries_the_app_skill() {
        let prompt = business_os_system_prompt();
        // The agent must be taught the load-bearing Business OS app conventions,
        // not just generic file tools.
        for marker in [
            "Business OS app",
            "mount(ctx)",
            "WebRTC",
            "kit",
            "in-memory projection",
        ] {
            assert!(
                prompt.contains(marker),
                "system prompt should mention `{marker}`"
            );
        }
    }

    #[test]
    fn coding_default_inherits_the_active_ctox_model() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let model = coding_default_model(temp.path());
        assert_eq!(model["id"], gateway_model(temp.path())["id"]);
        assert_eq!(
            model["api"].as_str(),
            Some("openai-responses"),
            "the coding default speaks the gateway's Responses shape"
        );
        let base_url = model["baseUrl"].as_str().unwrap_or_default();
        assert!(
            base_url.starts_with("http://") && base_url.ends_with(":12434/v1"),
            "coding default routes through the loopback gateway on :12434 (got {base_url})"
        );
        Ok(())
    }

    #[test]
    fn coding_model_preset_is_resolved_server_side_and_unknown_ids_fail() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        assert_eq!(resolve_coding_model_preset(temp.path(), "ctox")?, None);
        let error = resolve_coding_model_preset(temp.path(), "browser-forged")
            .unwrap_err()
            .to_string();
        assert!(error.contains("unavailable"));
        Ok(())
    }

    #[test]
    fn minimax_coding_plan_is_an_independent_account_preset() -> anyhow::Result<()> {
        use crate::execution::models::minimax_coding::{
            store_accounts, MiniMaxCodingAccount, MiniMaxCodingAccountsConfig,
            MiniMaxCodingEndpointProfile, MiniMaxSecretRef,
        };

        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let config = MiniMaxCodingAccountsConfig {
            accounts: vec![MiniMaxCodingAccount {
                id: "bulk-primary".to_owned(),
                disabled: false,
                models: vec!["MiniMax-M3".to_owned()],
                api_key_secret: MiniMaxSecretRef {
                    scope: "provider-subscriptions".to_owned(),
                    name: "bulk-primary-api-key".to_owned(),
                },
                endpoint_profile: MiniMaxCodingEndpointProfile::GlobalAnthropic,
            }],
            ..MiniMaxCodingAccountsConfig::default()
        };
        store_accounts(root, &config)?;

        // Config alone must not advertise an unusable account.
        assert_eq!(
            coding_model_capabilities(root)["presets"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        crate::secrets::write_secret_record(
            root,
            "provider-subscriptions",
            "bulk-primary-api-key",
            "test-secret",
            None,
            serde_json::json!({"provider":"minimax","access_mode":"coding_plan"}),
        )?;

        let capabilities = coding_model_capabilities(root);
        let preset = capabilities["presets"]
            .as_array()
            .and_then(|presets| {
                presets.iter().find(|preset| {
                    preset["model"]["provider"].as_str() == Some("ctox-minimax-coding")
                })
            })
            .context("MiniMax preset missing")?;
        let preset_id = preset["id"].as_str().context("preset id missing")?;
        let resolved = resolve_coding_model_preset(root, preset_id)?.context("route model")?;
        assert_eq!(resolved["id"], "MiniMax-M3");
        assert_eq!(resolved["ctoxRoute"]["accountId"], "bulk-primary");
        assert_ne!(resolved["provider"], "ctox_proxy");
        assert!(
            serde_json::to_string(&capabilities)?
                .find("test-secret")
                .is_none(),
            "public capability document must not expose secret material"
        );
        assert!(
            serde_json::to_string(&capabilities)?
                .find(BRIDGE_TOKEN_HEADER)
                .is_none(),
            "turn-local bridge authority must not exist in capability projection"
        );
        Ok(())
    }

    #[test]
    fn kimi_coding_plan_is_an_independent_account_preset() -> anyhow::Result<()> {
        use crate::execution::models::kimi_coding::{
            store_accounts, KimiCodingAccount, KimiCodingAccountsConfig, KimiCodingEndpointProfile,
            KimiSecretRef,
        };

        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let config = KimiCodingAccountsConfig {
            accounts: vec![KimiCodingAccount {
                id: "kimi-primary".to_owned(),
                disabled: false,
                models: vec!["k3[1m]".to_owned()],
                api_key_secret: KimiSecretRef {
                    scope: "provider-subscriptions".to_owned(),
                    name: "kimi-primary-api-key".to_owned(),
                },
                endpoint_profile: KimiCodingEndpointProfile::KimiCoding,
            }],
            ..KimiCodingAccountsConfig::default()
        };
        store_accounts(root, &config)?;

        assert_eq!(
            coding_model_capabilities(root)["presets"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        crate::secrets::write_secret_record(
            root,
            "provider-subscriptions",
            "kimi-primary-api-key",
            "test-secret",
            None,
            serde_json::json!({"provider":"kimi","access_mode":"coding_plan"}),
        )?;

        let capabilities = coding_model_capabilities(root);
        let preset = capabilities["presets"]
            .as_array()
            .and_then(|presets| {
                presets
                    .iter()
                    .find(|preset| preset["model"]["provider"].as_str() == Some("ctox-kimi-coding"))
            })
            .context("Kimi coding preset missing")?;
        let preset_id = preset["id"].as_str().context("preset id missing")?;
        let resolved = resolve_coding_model_preset(root, preset_id)?.context("route model")?;
        assert_eq!(resolved["id"], "k3[1m]");
        assert_eq!(resolved["contextWindow"], 1_048_576);
        assert_eq!(resolved["ctoxRoute"]["accountId"], "kimi-primary");
        assert_ne!(resolved["provider"], "ctox_proxy");
        let public = serde_json::to_string(&capabilities)?;
        assert!(!public.contains("test-secret"));
        assert!(!public.contains(BRIDGE_TOKEN_HEADER));
        Ok(())
    }

    #[test]
    fn minimax_preset_drives_a_real_pi_edit_through_the_selected_account() -> anyhow::Result<()> {
        use crate::execution::models::minimax_coding::{
            store_accounts, MiniMaxCodingAccount, MiniMaxCodingAccountsConfig,
            MiniMaxCodingEndpointProfile, MiniMaxSecretRef,
        };

        if !node_available() {
            eprintln!("SKIP: `node` not on PATH");
            return Ok(());
        }
        let dist = sidecar_dist_path(&repo_root());
        if !dist.exists() {
            eprintln!("SKIP: pi-sidecar bundle not built ({})", dist.display());
            return Ok(());
        }
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        seed_unrelated_main_model(root)?;
        store_accounts(
            root,
            &MiniMaxCodingAccountsConfig {
                accounts: vec![MiniMaxCodingAccount {
                    id: "minimax-smoke-account".to_owned(),
                    disabled: false,
                    models: vec!["MiniMax-M3".to_owned()],
                    api_key_secret: MiniMaxSecretRef {
                        scope: "provider-subscriptions".to_owned(),
                        name: "minimax-smoke-key".to_owned(),
                    },
                    endpoint_profile: MiniMaxCodingEndpointProfile::GlobalAnthropic,
                }],
                ..MiniMaxCodingAccountsConfig::default()
            },
        )?;
        crate::secrets::write_secret_record(
            root,
            "provider-subscriptions",
            "minimax-smoke-key",
            "minimax-smoke-secret-must-not-leak",
            None,
            serde_json::json!({"test":true}),
        )?;
        let preset_id = coding_model_capabilities(root)["presets"]
            .as_array()
            .and_then(|presets| {
                presets.iter().find(|preset| {
                    preset["model"]["provider"].as_str() == Some("ctox-minimax-coding")
                })
            })
            .and_then(|preset| preset["id"].as_str())
            .context("MiniMax smoke preset")?
            .to_owned();
        let (upstream, worker) =
            spawn_anthropic_edit_upstream("MiniMax-M3", "minimax-smoke-secret-must-not-leak")?;
        let evidence =
            run_coding_preset_smoke_inner(root, &dist, &preset_id, None, Some(&upstream), None)?;
        worker.join().expect("fake MiniMax upstream");

        assert_eq!(evidence["provider"], "minimax_coding_plan");
        assert_eq!(evidence["model"], "MiniMax-M3");
        assert_eq!(evidence["main_model_unchanged"], true);
        assert_eq!(
            crate::inference::runtime_env::effective_chat_model(root).as_deref(),
            Some("main-model-must-stay-selected")
        );
        let rendered = evidence.to_string();
        assert!(!rendered.contains("minimax-smoke-account"));
        assert!(!rendered.contains("minimax-smoke-secret-must-not-leak"));
        assert!(!rendered.contains(&upstream));
        Ok(())
    }

    #[test]
    fn kimi_preset_drives_a_real_pi_edit_through_the_selected_account() -> anyhow::Result<()> {
        use crate::execution::models::kimi_coding::{
            store_accounts, KimiCodingAccount, KimiCodingAccountsConfig, KimiCodingEndpointProfile,
            KimiSecretRef,
        };

        if !node_available() {
            eprintln!("SKIP: `node` not on PATH");
            return Ok(());
        }
        let dist = sidecar_dist_path(&repo_root());
        if !dist.exists() {
            eprintln!("SKIP: pi-sidecar bundle not built ({})", dist.display());
            return Ok(());
        }
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        seed_unrelated_main_model(root)?;
        store_accounts(
            root,
            &KimiCodingAccountsConfig {
                accounts: vec![KimiCodingAccount {
                    id: "kimi-smoke-account".to_owned(),
                    disabled: false,
                    models: vec!["k3[1m]".to_owned()],
                    api_key_secret: KimiSecretRef {
                        scope: "provider-subscriptions".to_owned(),
                        name: "kimi-smoke-key".to_owned(),
                    },
                    endpoint_profile: KimiCodingEndpointProfile::KimiCoding,
                }],
                ..KimiCodingAccountsConfig::default()
            },
        )?;
        crate::secrets::write_secret_record(
            root,
            "provider-subscriptions",
            "kimi-smoke-key",
            "kimi-smoke-secret-must-not-leak",
            None,
            serde_json::json!({"test":true}),
        )?;
        let preset_id = coding_model_capabilities(root)["presets"]
            .as_array()
            .and_then(|presets| {
                presets
                    .iter()
                    .find(|preset| preset["model"]["provider"].as_str() == Some("ctox-kimi-coding"))
            })
            .and_then(|preset| preset["id"].as_str())
            .context("Kimi smoke preset")?
            .to_owned();
        let (upstream, worker) =
            spawn_anthropic_edit_upstream("k3[1m]", "kimi-smoke-secret-must-not-leak")?;
        let evidence =
            run_coding_preset_smoke_inner(root, &dist, &preset_id, None, Some(&upstream), None)?;
        worker.join().expect("fake Kimi upstream");

        assert_eq!(evidence["provider"], "kimi_coding_plan");
        assert_eq!(evidence["model"], "k3[1m]");
        assert_eq!(evidence["main_model_unchanged"], true);
        assert_eq!(
            crate::inference::runtime_env::effective_chat_model(root).as_deref(),
            Some("main-model-must-stay-selected")
        );
        let rendered = evidence.to_string();
        assert!(!rendered.contains("kimi-smoke-account"));
        assert!(!rendered.contains("kimi-smoke-secret-must-not-leak"));
        assert!(!rendered.contains(&upstream));
        Ok(())
    }

    #[test]
    fn inherited_ctox_proxy_route_keeps_credentials_and_configuration_native() -> anyhow::Result<()>
    {
        use crate::execution::models::runtime_env;
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let settings = BTreeMap::from([
            ("CTOX_CHAT_SOURCE".to_owned(), "api".to_owned()),
            ("CTOX_API_PROVIDER".to_owned(), "ctox_proxy".to_owned()),
            ("CTOX_CHAT_MODEL".to_owned(), "MiniMax-M3".to_owned()),
            ("CTOX_CHAT_MODEL_BASE".to_owned(), "MiniMax-M3".to_owned()),
            (
                "CTOX_UPSTREAM_BASE_URL".to_owned(),
                "https://llm.ctox.dev".to_owned(),
            ),
            (
                "CTOX_LLM_PROXY_API_KEY".to_owned(),
                "selected-proxy-secret".to_owned(),
            ),
            (
                "MINIMAX_API_KEY".to_owned(),
                "unrelated-minimax-secret".to_owned(),
            ),
        ]);
        runtime_env::save_runtime_env_map(root, &settings)?;
        let before = runtime_env::load_runtime_env_map(root)?;
        let prepared = prepare_coding_turn_model(root, None, None, false)?;
        assert_eq!(prepared.provider, "ctox_proxy");
        assert_eq!(prepared.model_id, "MiniMax-M3");
        assert!(prepared.coding_plan_bridge.is_some());
        let public = prepared.model.to_string();
        assert!(!public.contains("selected-proxy-secret"));
        assert!(!public.contains("unrelated-minimax-secret"));
        assert!(!public.contains("llm.ctox.dev"));
        assert!(!public.contains(":12434"));
        assert!(prepared.model.get("ctoxRoute").is_none());
        assert_eq!(runtime_env::load_runtime_env_map(root)?, before);
        drop(prepared);
        Ok(())
    }

    #[test]
    fn inherited_minimax_uses_chat_wire_and_its_own_credential() -> anyhow::Result<()> {
        use crate::execution::models::runtime_env;
        let temp = tempfile::tempdir()?;
        let settings = BTreeMap::from([
            ("CTOX_CHAT_SOURCE".to_owned(), "api".to_owned()),
            ("CTOX_API_PROVIDER".to_owned(), "minimax".to_owned()),
            ("CTOX_CHAT_MODEL".to_owned(), "MiniMax-M3".to_owned()),
            ("CTOX_CHAT_MODEL_BASE".to_owned(), "MiniMax-M3".to_owned()),
            (
                "CTOX_UPSTREAM_BASE_URL".to_owned(),
                "https://api.minimax.io".to_owned(),
            ),
            (
                "MINIMAX_API_KEY".to_owned(),
                "native-minimax-key".to_owned(),
            ),
            (
                "CTOX_LLM_PROXY_API_KEY".to_owned(),
                "unrelated-proxy-key".to_owned(),
            ),
        ]);
        runtime_env::save_runtime_env_map(temp.path(), &settings)?;
        let route = resolve_inherited_coding_route(temp.path())?;
        assert_eq!(route.credential_key, "MINIMAX_API_KEY");
        assert_eq!(route.base_url, "https://api.minimax.io/v1");
        let evidence = inherited_coding_route_status(temp.path())?;
        assert_eq!(evidence["provider"], "minimax");
        assert_eq!(evidence["wire_api"], "openai-completions");
        assert_eq!(evidence["upstream_origin"], "https://api.minimax.io");
        assert!(!evidence.to_string().contains("key"));
        let prepared = prepare_coding_turn_model(temp.path(), None, None, false)?;
        assert_eq!(prepared.model["api"], "openai-completions");
        assert_eq!(prepared.model["compat"]["maxTokensField"], "max_tokens");
        assert!(!prepared.model.to_string().contains("native-minimax-key"));
        Ok(())
    }

    #[test]
    fn inherited_minimax_route_drives_real_pi_tools_through_native_bridge() -> anyhow::Result<()> {
        let dist = sidecar_dist_path(&repo_root());
        anyhow::ensure!(
            node_available() && dist.exists(),
            "real Pi regression requires Node and built sidecar bundle"
        );
        let temp = tempfile::tempdir()?;
        let settings = BTreeMap::from([
            ("CTOX_CHAT_SOURCE".to_owned(), "api".to_owned()),
            ("CTOX_API_PROVIDER".to_owned(), "minimax".to_owned()),
            ("CTOX_CHAT_MODEL".to_owned(), "MiniMax-M3".to_owned()),
            ("CTOX_CHAT_MODEL_BASE".to_owned(), "MiniMax-M3".to_owned()),
            (
                "CTOX_UPSTREAM_BASE_URL".to_owned(),
                "https://api.minimax.io".to_owned(),
            ),
            (
                "MINIMAX_API_KEY".to_owned(),
                "fixture-main-secret".to_owned(),
            ),
        ]);
        crate::execution::models::runtime_env::save_runtime_env_map(temp.path(), &settings)?;
        let mut route = resolve_inherited_coding_route(temp.path())?;
        assert_eq!(route.api, "openai-completions");
        let upstream =
            Server::http("127.0.0.1:0").map_err(|error| anyhow::anyhow!(error.to_string()))?;
        // Private test seam changes only transport destination after resolving
        // the real provider, model, credential selector and protocol.
        route.base_url = format!(
            "http://{}/v1",
            upstream.server_addr().to_ip().context("fixture IP")?
        );
        let worker = std::thread::spawn(move || {
            for turn in 0..2 {
                let mut request = upstream
                    .recv_timeout(Duration::from_secs(15))
                    .unwrap()
                    .expect("real Pi request");
                assert_eq!(request.url(), "/v1/chat/completions");
                assert!(request.headers().iter().any(|header| header
                    .field
                    .as_str()
                    .as_str()
                    .eq_ignore_ascii_case("authorization")
                    && header.value.as_str() == "Bearer fixture-main-secret"));
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                let body: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(body["model"], "MiniMax-M3");
                assert!(body.get("store").is_none());
                let (delta, finish) = if turn == 0 {
                    (
                        serde_json::json!({"role":"assistant", "tool_calls":[{"index":0,"id":"write-1","type":"function","function":{"name":"write","arguments":"{\"path\":\"index.js\",\"content\":\"export const v = 2;\\n\"}"}}]}),
                        "tool_calls",
                    )
                } else {
                    assert!(body["messages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|message| message["role"] == "tool"));
                    (
                        serde_json::json!({"role":"assistant", "content":"Done"}),
                        "stop",
                    )
                };
                let chunk = serde_json::json!({"id":"fixture", "object":"chat.completion.chunk", "created":1, "model":"MiniMax-M3", "choices":[{"index":0,"delta":delta,"finish_reason":null}]});
                let end = serde_json::json!({"id":"fixture", "object":"chat.completion.chunk", "created":1, "model":"MiniMax-M3", "choices":[{"index":0,"delta":{},"finish_reason":finish}], "usage":{"prompt_tokens":10,"completion_tokens":10,"total_tokens":20}});
                request
                    .respond(
                        Response::from_string(format!(
                            "data: {chunk}\n\ndata: {end}\n\ndata: [DONE]\n\n"
                        ))
                        .with_header(
                            Header::from_bytes("content-type", "text/event-stream").unwrap(),
                        ),
                    )
                    .unwrap();
            }
        });
        let prepared = prepare_inherited_coding_model_route(temp.path(), route)?;
        let response = run_pi_turn(
            &dist,
            &serde_json::json!({
                "id":"main-route-fixture", "prompt":"Write index.js with v = 2", "files":{"index.js":"export const v = 1;\n"},
                "maxAssistantTurns":3, "tools":["write"], "model":prepared.model,
            }),
            false,
        )?;
        worker.join().expect("provider fixture assertions");
        assert_eq!(response["ok"], true, "{response}");
        assert!(response["snapshot"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["content"] == "export const v = 2;\n"));
        drop(prepared.coding_plan_bridge);
        Ok(())
    }

    #[test]
    fn gateway_model_defers_private_route_resolution_to_the_turn_owner() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let model = gateway_model(temp.path());
        let base_url = model["baseUrl"].as_str().unwrap_or_default();
        assert!(
            base_url == "http://127.0.0.1:1/v1",
            "public descriptor must not advertise the retired gateway (got {base_url})"
        );
        assert_eq!(model["ctoxRoute"]["kind"], "inherit_ctox");
        assert_eq!(
            model["api"].as_str(),
            Some("openai-responses"),
            "uses pi-ai's OpenAI Responses provider"
        );
        assert!(
            model["id"]
                .as_str()
                .map(|id| !id.is_empty())
                .unwrap_or(false),
            "an active model id is resolved from the gateway config"
        );
        Ok(())
    }
}
