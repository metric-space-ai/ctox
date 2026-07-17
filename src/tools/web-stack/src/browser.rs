use anyhow::Context;
use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use std::fs;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::ChildStdin;
use std::process::ChildStdout;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use url::Url;

pub(crate) const DEFAULT_REFERENCE_RELATIVE_DIR: &str = "runtime/browser/interactive-reference";
const LOCAL_PLAYWRIGHT_BROWSERS_RELATIVE_DIR: &str = "ms-playwright";
const MINIMUM_NODE_MAJOR: u64 = 18;

#[derive(Debug, Clone, Serialize)]
struct ToolStatus {
    available: bool,
    path: Option<String>,
    version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct BrowserDoctorReport {
    ok: bool,
    reference_dir: PathBuf,
    package_json_exists: bool,
    node_modules_exists: bool,
    minimum_node_major: u64,
    node_major: Option<u64>,
    node_version_compatible: bool,
    runner_dependency_declared: bool,
    runner_dependency_installed: bool,
    runner_browser_cache_dir: PathBuf,
    runner_browser_installed: bool,
    chromium_fallback_executable: Option<String>,
    toolchain: serde_json::Value,
    smoke: BrowserSmokeReport,
    automation_ready: bool,
}

#[derive(Debug, Clone, Serialize)]
struct BrowserSmokeReport {
    ran: bool,
    ok: bool,
    timeout_ms: u64,
    stdout: Option<String>,
    stderr: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct BrowserInstallReport {
    ok: bool,
    reference_dir: PathBuf,
    package_json_created: bool,
    npm_install_ran: bool,
    browser_install_ran: bool,
}

#[derive(Debug, Clone, Default)]
pub struct BrowserPrepareOptions {
    pub dir: Option<PathBuf>,
    pub install_reference: bool,
    pub install_browser: bool,
    pub skip_npm_install: bool,
}

#[derive(Debug, Clone)]
pub struct BrowserAutomationRequest {
    pub dir: Option<PathBuf>,
    pub timeout_ms: Option<u64>,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct BrowserCaptureRequest {
    pub dir: Option<PathBuf>,
    pub out_dir: Option<PathBuf>,
    pub timeout_ms: Option<u64>,
    pub url: String,
}

#[derive(Debug, Default)]
struct BrowserAutomationDirective {
    timeout_ms: Option<u64>,
}

pub fn handle_browser_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "doctor" => {
            let reference_dir = resolve_reference_dir(root, &args[1..]);
            let report = build_doctor_report(&reference_dir)?;
            print_json(&serde_json::to_value(report)?)
        }
        "install-reference" => {
            let reference_dir = resolve_reference_dir(root, &args[1..]);
            let install_browser = args.iter().any(|arg| arg == "--install-browser");
            let skip_npm_install = args.iter().any(|arg| arg == "--skip-npm-install");
            let report = install_reference(&reference_dir, !skip_npm_install, install_browser)?;
            print_json(&serde_json::to_value(report)?)
        }
        "bootstrap" => {
            let reference_dir = resolve_reference_dir(root, &args[1..]);
            print_json(&bootstrap_payload(&reference_dir))
        }
        _ => anyhow::bail!(
            "usage:\n  ctox browser doctor [--dir <path>]\n  ctox browser install-reference [--dir <path>] [--skip-npm-install] [--install-browser]\n  ctox browser bootstrap [--dir <path>]"
        ),
    }
}

pub fn browser_doctor_report(root: &Path, dir: Option<PathBuf>) -> Result<Value> {
    let reference_dir = browser_reference_dir(root, dir);
    let report = build_doctor_report(&reference_dir)?;
    Ok(serde_json::to_value(report)?)
}

pub fn prepare_browser_environment(
    root: &Path,
    options: &BrowserPrepareOptions,
) -> Result<serde_json::Value> {
    let reference_dir = browser_reference_dir(root, options.dir.clone());
    let install_report = if options.install_reference || options.install_browser {
        Some(serde_json::to_value(install_reference(
            &reference_dir,
            !options.skip_npm_install,
            options.install_browser,
        )?)?)
    } else {
        None
    };
    Ok(json!({
        "ok": true,
        "tool": "ctox_browser_prepare",
        "install": install_report,
        "doctor": build_doctor_report(&reference_dir)?,
        "bootstrap": bootstrap_payload(&reference_dir),
    }))
}

pub fn read_browser_automation_source(script_file: Option<&Path>) -> Result<String> {
    if let Some(script_file) = script_file {
        return fs::read_to_string(script_file).with_context(|| {
            format!(
                "failed to read browser automation source {}",
                script_file.display()
            )
        });
    }

    let mut source = String::new();
    io::stdin()
        .read_to_string(&mut source)
        .context("failed to read browser automation source from stdin")?;
    if source.trim().is_empty() {
        anyhow::bail!("browser automation expects JavaScript on stdin or via --script-file <path>");
    }
    Ok(source)
}

pub fn run_browser_automation(root: &Path, request: &BrowserAutomationRequest) -> Result<Value> {
    let reference_dir = browser_reference_dir(root, request.dir.clone());
    fs::create_dir_all(&reference_dir).with_context(|| {
        format!(
            "failed to create browser automation reference dir {}",
            reference_dir.display()
        )
    })?;
    let _ = ensure_reference_package_json(&reference_dir)?;
    ensure_humanlike_module(&reference_dir)?;
    ensure_stealth_init_module(&reference_dir)?;
    let _doctor = ensure_browser_automation_ready(&reference_dir, "browser automation")?;
    let Some(node_path) = find_command_on_path("node") else {
        anyhow::bail!("node is required for browser automation");
    };

    let (directive, source) = parse_browser_automation_source(&request.source)?;
    let timeout_ms = request
        .timeout_ms
        .or(directive.timeout_ms)
        .unwrap_or(30_000)
        .clamp(1_000, 300_000);
    let fallback_executable =
        find_browser_executable(&reference_dir).map(|value| value.display().to_string());
    let runner_path = reference_dir.join(format!(
        ".ctox-browser-run-{}-{}.mjs",
        std::process::id(),
        unix_ts()
    ));
    let runner_source =
        build_browser_runner_script(&source, timeout_ms, fallback_executable.as_deref())?;
    fs::write(&runner_path, runner_source)
        .with_context(|| format!("failed to write {}", runner_path.display()))?;

    let mut command = Command::new(&node_path);
    command
        .current_dir(&reference_dir)
        .env(
            "PLAYWRIGHT_BROWSERS_PATH",
            playwright_browser_cache_dir(&reference_dir),
        )
        .arg(&runner_path);
    let output = command_output_with_timeout(
        command,
        Duration::from_millis(timeout_ms.saturating_add(5_000)),
    )
    .with_context(|| {
        format!(
            "failed to launch browser automation runtime with {}",
            node_path.display()
        )
    });
    let _ = fs::remove_file(&runner_path);
    let output = output?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        anyhow::bail!("browser automation runtime failed: {detail}");
    }

    let payload: Value = serde_json::from_str(&stdout).with_context(|| {
        format!(
            "browser automation runtime produced invalid json: {}",
            trim_text(&stdout, 400)
        )
    })?;
    record_browser_detection_signal(root, &payload);
    Ok(payload)
}

fn record_browser_detection_signal(root: &Path, payload: &Value) {
    let Some(detection) = payload.get("detection") else {
        return;
    };
    let markers = detection
        .get("markers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if markers.is_empty() {
        return;
    }
    let probe_url = detection.get("url").and_then(Value::as_str);
    #[cfg(feature = "full")]
    crate::unlock::record_signal_lossy(
        root,
        "browser_automation",
        probe_url,
        json!({
            "reason": "browser_challenge_detected",
            "markers": markers,
            "title": detection.get("title").cloned().unwrap_or(Value::Null),
            "automation_ok": payload.get("ok").cloned().unwrap_or(Value::Null),
        }),
    );
    #[cfg(not(feature = "full"))]
    let _ = (root, probe_url, markers);
}

pub fn capture_browser_transport(root: &Path, request: &BrowserCaptureRequest) -> Result<Value> {
    let reference_dir = browser_reference_dir(root, request.dir.clone());
    fs::create_dir_all(&reference_dir).with_context(|| {
        format!(
            "failed to create browser automation reference dir {}",
            reference_dir.display()
        )
    })?;
    let _ = ensure_reference_package_json(&reference_dir)?;
    ensure_humanlike_module(&reference_dir)?;
    ensure_stealth_init_module(&reference_dir)?;
    let _doctor = ensure_browser_automation_ready(&reference_dir, "browser capture")?;
    let Some(node_path) = find_command_on_path("node") else {
        anyhow::bail!("node is required for browser capture");
    };
    let (browser_source, browser_executable) = find_capture_browser_executable(&reference_dir).context(
        "browser capture requires Playwright Chrome for Testing; run `ctox web browser-prepare --install-browser`",
    )?;

    let timeout_ms = request.timeout_ms.unwrap_or(45_000).clamp(5_000, 300_000);
    let out_dir = resolve_root_relative_path(
        root,
        request.out_dir.clone().unwrap_or_else(|| {
            root.join("runtime")
                .join("browser")
                .join("captures")
                .join(format!("capture-{}", unix_ts()))
        }),
    );
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create browser capture dir {}", out_dir.display()))?;
    let out_dir = fs::canonicalize(&out_dir).with_context(|| {
        format!(
            "failed to canonicalize browser capture dir {}",
            out_dir.display()
        )
    })?;
    let profile_dir = out_dir.join("chrome-profile");
    fs::create_dir_all(&profile_dir).with_context(|| {
        format!(
            "failed to create browser profile dir {}",
            profile_dir.display()
        )
    })?;
    let chrome_stdout_path = out_dir.join("chrome.stdout.log");
    let chrome_stderr_path = out_dir.join("chrome.stderr.log");
    let netlog_path = out_dir.join("chrome-netlog.json");
    let stdout_file = fs::File::create(&chrome_stdout_path)
        .with_context(|| format!("failed to create {}", chrome_stdout_path.display()))?;
    let stderr_file = fs::File::create(&chrome_stderr_path)
        .with_context(|| format!("failed to create {}", chrome_stderr_path.display()))?;

    let headless_without_gui = looks_headless_without_browser_session();
    let headless_capture = true;
    let chrome_launch_args = capture_chrome_extra_args(headless_capture, cfg!(target_os = "linux"));
    let mut chrome_command = Command::new(&browser_executable);
    chrome_command
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("--remote-debugging-address=127.0.0.1")
        .arg("--remote-debugging-port=0")
        .arg("--no-first-run")
        .arg("--no-default-browser-check");
    for arg in &chrome_launch_args {
        chrome_command.arg(arg);
    }
    let mut chrome = chrome_command
        .arg(format!("--log-net-log={}", netlog_path.display()))
        .arg("--net-log-capture-mode=Everything")
        .arg("about:blank")
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .with_context(|| {
            format!(
                "failed to launch browser capture chrome {}",
                browser_executable.display()
            )
        })?;

    let cdp_url = match wait_for_devtools_url(&profile_dir, Duration::from_secs(15)) {
        Ok(value) => value,
        Err(err) => {
            let _ = chrome.kill();
            let _ = chrome.wait();
            return Err(err);
        }
    };

    let runner_path = reference_dir.join(format!(
        ".ctox-browser-capture-{}-{}.mjs",
        std::process::id(),
        unix_ts()
    ));
    let runner_source =
        build_browser_capture_runner_script(&cdp_url, &request.url, &out_dir, timeout_ms)?;
    fs::write(&runner_path, runner_source)
        .with_context(|| format!("failed to write {}", runner_path.display()))?;

    let output = Command::new(&node_path)
        .current_dir(&reference_dir)
        .arg(&runner_path)
        .output()
        .with_context(|| {
            format!(
                "failed to launch browser capture runtime with {}",
                node_path.display()
            )
        });
    let _ = fs::remove_file(&runner_path);
    let _ = chrome.kill();
    let _ = chrome.wait();
    let output = output?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        anyhow::bail!("browser capture runtime failed: {detail}");
    }

    let mut payload: Value = serde_json::from_str(&stdout).with_context(|| {
        format!(
            "browser capture runtime produced invalid json: {}",
            trim_text(&stdout, 400)
        )
    })?;
    if let Some(object) = payload.as_object_mut() {
        object.insert(
            "browser_source".to_string(),
            Value::String(browser_source.to_string()),
        );
        object.insert(
            "browser_executable".to_string(),
            Value::String(browser_executable.display().to_string()),
        );
        object.insert("cdp_url".to_string(), Value::String(cdp_url));
        object.insert(
            "chrome_stdout_log".to_string(),
            Value::String(chrome_stdout_path.display().to_string()),
        );
        object.insert(
            "chrome_stderr_log".to_string(),
            Value::String(chrome_stderr_path.display().to_string()),
        );
        object.insert(
            "netlog_path".to_string(),
            Value::String(netlog_path.display().to_string()),
        );
        object.insert(
            "profile_dir".to_string(),
            Value::String(profile_dir.display().to_string()),
        );
        object.insert(
            "reference_dir".to_string(),
            Value::String(reference_dir.display().to_string()),
        );
        object.insert(
            "headless_without_gui".to_string(),
            Value::Bool(headless_without_gui),
        );
        object.insert("headless".to_string(), Value::Bool(headless_capture));
        object.insert(
            "chrome_launch_args".to_string(),
            Value::Array(
                chrome_launch_args
                    .into_iter()
                    .map(|arg| Value::String(arg.to_string()))
                    .collect(),
            ),
        );
    }
    Ok(payload)
}

fn looks_headless_without_browser_session() -> bool {
    if cfg!(target_os = "macos") {
        return false;
    }
    std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none()
}

fn capture_chrome_extra_args(headless_without_gui: bool, linux: bool) -> Vec<&'static str> {
    let mut args = Vec::new();
    if headless_without_gui {
        args.push("--headless=new");
        args.push("--disable-gpu");
        args.push("--disable-dev-shm-usage");
    }
    if linux {
        args.push("--no-sandbox");
    }
    args
}

fn build_doctor_report(reference_dir: &Path) -> Result<BrowserDoctorReport> {
    let package_json_exists = reference_dir.join("package.json").exists();
    let node_modules_exists = reference_dir.join("node_modules").is_dir();
    let runner_dependency_declared = read_runner_dependency_declared(reference_dir)?;
    let runner_dependency_installed = reference_dir
        .join("node_modules")
        .join("patchright")
        .join("package.json")
        .is_file();
    let runner_browser_cache_dir = playwright_browser_cache_dir(reference_dir);
    let chromium_fallback_executable =
        find_browser_executable(reference_dir).map(|value| value.display().to_string());
    let runner_browser_installed = chromium_fallback_executable.is_some();
    let node = detect_tool("node", &["--version"]);
    let npm = detect_tool("npm", &["--version"]);
    let npx = detect_tool("npx", &["--version"]);
    let node_major = node.version.as_deref().and_then(parse_node_major_version);
    let node_version_compatible = node_major
        .map(|major| major >= MINIMUM_NODE_MAJOR)
        .unwrap_or(false);
    let ok = node.available && npm.available && npx.available;
    let smoke =
        if ok && node_version_compatible && runner_dependency_installed && runner_browser_installed
        {
            run_browser_smoke(reference_dir, chromium_fallback_executable.as_deref())
        } else {
            BrowserSmokeReport {
                ran: false,
                ok: false,
                timeout_ms: 8_000,
                stdout: None,
                stderr: None,
                error: Some("skipped because browser prerequisites are incomplete".to_string()),
            }
        };
    let automation_ready =
        ok && node_version_compatible && runner_dependency_installed && runner_browser_installed;
    Ok(BrowserDoctorReport {
        ok,
        reference_dir: reference_dir.to_path_buf(),
        package_json_exists,
        node_modules_exists,
        minimum_node_major: MINIMUM_NODE_MAJOR,
        node_major,
        node_version_compatible,
        runner_dependency_declared,
        runner_dependency_installed,
        runner_browser_cache_dir,
        runner_browser_installed,
        chromium_fallback_executable,
        toolchain: json!({
            "node": node,
            "npm": npm,
            "npx": npx,
        }),
        smoke,
        automation_ready,
    })
}

fn ensure_browser_automation_ready(
    reference_dir: &Path,
    context_label: &str,
) -> Result<BrowserDoctorReport> {
    let mut doctor = build_doctor_report(reference_dir)?;
    if doctor.automation_ready {
        return Ok(doctor);
    }

    let run_npm_install = !doctor.runner_dependency_declared || !doctor.runner_dependency_installed;
    let install_browser = !doctor.runner_browser_installed;
    if run_npm_install || install_browser {
        install_reference(reference_dir, run_npm_install, install_browser)?;
        doctor = build_doctor_report(reference_dir)?;
        if doctor.automation_ready {
            return Ok(doctor);
        }
    }

    anyhow::bail!(
        "{} runtime is not ready for {}. Run `ctox web browser-prepare --dir {} --install-reference [--install-browser]` first. Doctor: {}",
        context_label,
        reference_dir.display(),
        reference_dir.display(),
        serde_json::to_string(&doctor).unwrap_or_else(|_| "{}".to_string())
    );
}

fn run_browser_smoke(
    reference_dir: &Path,
    chromium_fallback_executable: Option<&str>,
) -> BrowserSmokeReport {
    // Cold-start of Patchright/Chromium on macOS can exceed 10s the first
    // time after a Gatekeeper quarantine refresh. 20s leaves headroom while
    // still failing fast on a real install regression.
    const TIMEOUT_MS: u64 = 20_000;
    let Some(node_path) = find_command_on_path("node") else {
        return BrowserSmokeReport {
            ran: true,
            ok: false,
            timeout_ms: TIMEOUT_MS,
            stdout: None,
            stderr: None,
            error: Some("node was not found on PATH".to_string()),
        };
    };
    let encoded_executable = match serde_json::to_string(&chromium_fallback_executable) {
        Ok(value) => value,
        Err(err) => {
            return BrowserSmokeReport {
                ran: true,
                ok: false,
                timeout_ms: TIMEOUT_MS,
                stdout: None,
                stderr: None,
                error: Some(format!("failed to encode browser executable: {err}")),
            };
        }
    };
    let smoke_source = format!(
        r#"
const fallbackExecutable = {encoded_executable};
const {{ chromium }} = await import("patchright");
const launchOptions = {{ headless: true }};
if (fallbackExecutable) {{
  launchOptions.executablePath = fallbackExecutable;
}}
const browser = await chromium.launch(launchOptions);
try {{
  const page = await browser.newPage();
  await page.goto("data:text/html,<title>ctox-browser-smoke</title><button data-testid='ready'>ready</button>", {{ waitUntil: "domcontentloaded" }});
  const text = await page.getByTestId("ready").textContent();
  await page.screenshot({{ type: "png" }});
  if (text !== "ready") {{
    throw new Error(`unexpected smoke text: ${{text}}`);
  }}
  console.log(JSON.stringify({{ ok: true, title: await page.title() }}));
}} finally {{
  await browser.close();
}}
"#
    );
    let mut command = Command::new(&node_path);
    command
        .current_dir(reference_dir)
        .env(
            "PLAYWRIGHT_BROWSERS_PATH",
            playwright_browser_cache_dir(reference_dir),
        )
        .arg("--input-type=module")
        .arg("-e")
        .arg(smoke_source);

    match command_output_with_timeout(command, Duration::from_millis(TIMEOUT_MS)) {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            BrowserSmokeReport {
                ran: true,
                ok: output.status.success(),
                timeout_ms: TIMEOUT_MS,
                stdout: (!stdout.is_empty()).then_some(trim_text(&stdout, 800)),
                stderr: (!stderr.is_empty()).then_some(trim_text(&stderr, 800)),
                error: (!output.status.success()).then(|| format!("exit status {}", output.status)),
            }
        }
        Err(err) => BrowserSmokeReport {
            ran: true,
            ok: false,
            timeout_ms: TIMEOUT_MS,
            stdout: None,
            stderr: None,
            error: Some(err.to_string()),
        },
    }
}

fn install_reference(
    reference_dir: &Path,
    run_npm_install: bool,
    install_browser: bool,
) -> Result<BrowserInstallReport> {
    if run_npm_install || install_browser {
        ensure_node_runtime_compatible()?;
    }
    fs::create_dir_all(reference_dir).with_context(|| {
        format!(
            "failed to create interactive browser reference dir {}",
            reference_dir.display()
        )
    })?;
    let package_json_created = ensure_reference_package_json(reference_dir)?;
    ensure_humanlike_module(reference_dir)?;
    ensure_stealth_init_module(reference_dir)?;
    if run_npm_install {
        run_command(
            reference_dir,
            "npm",
            &["install", "patchright"],
            "failed to install patchright reference",
        )?;
    }
    if install_browser {
        let browser_cache_dir = playwright_browser_cache_dir(reference_dir);
        run_command_with_env(
            reference_dir,
            "npx",
            &["patchright", "install", "chromium"],
            &[("PLAYWRIGHT_BROWSERS_PATH", browser_cache_dir.as_path())],
            "failed to install Patchright chromium browser",
        )?;
    }
    Ok(BrowserInstallReport {
        ok: true,
        reference_dir: reference_dir.to_path_buf(),
        package_json_created,
        npm_install_ran: run_npm_install,
        browser_install_ran: install_browser,
    })
}

fn ensure_humanlike_module(reference_dir: &Path) -> Result<()> {
    let target = reference_dir.join("humanlike.mjs");
    let source = include_str!("../assets/humanlike.mjs");
    let needs_write = match fs::read_to_string(&target) {
        Ok(existing) => existing != source,
        Err(_) => true,
    };
    if needs_write {
        fs::write(&target, source)
            .with_context(|| format!("failed to write {}", target.display()))?;
    }
    Ok(())
}

fn ensure_stealth_init_module(reference_dir: &Path) -> Result<()> {
    let target = reference_dir.join("stealth_init.js");
    let source = include_str!("../assets/stealth_init.js");
    let needs_write = match fs::read_to_string(&target) {
        Ok(existing) => existing != source,
        Err(_) => true,
    };
    if needs_write {
        fs::write(&target, source)
            .with_context(|| format!("failed to write {}", target.display()))?;
    }
    Ok(())
}

fn ensure_reference_package_json(reference_dir: &Path) -> Result<bool> {
    let package_json_path = reference_dir.join("package.json");
    if package_json_path.exists() {
        return Ok(false);
    }
    let package_json = json!({
        "name": "ctox-interactive-browser-reference",
        "private": true,
        "type": "module",
        "description": "CTOX-owned Patchright runtime workspace for browser automation.",
        "scripts": {
            "doctor": "node -e \"import('patchright').then(() => console.log('patchright import ok')).catch((error) => { console.error(error); process.exit(1); })\"",
            "install:chromium": "patchright install chromium"
        },
        "dependencies": {
            "patchright": "^1.55.0"
        }
    });
    fs::write(
        &package_json_path,
        serde_json::to_vec_pretty(&package_json)?,
    )
    .with_context(|| format!("failed to write {}", package_json_path.display()))?;
    Ok(true)
}

fn read_runner_dependency_declared(reference_dir: &Path) -> Result<bool> {
    let package_json_path = reference_dir.join("package.json");
    if !package_json_path.exists() {
        return Ok(false);
    }
    let raw = fs::read(&package_json_path)
        .with_context(|| format!("failed to read {}", package_json_path.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&raw).context("failed to parse browser reference package.json")?;
    Ok(value
        .get("dependencies")
        .and_then(|value| value.get("patchright"))
        .and_then(serde_json::Value::as_str)
        .is_some())
}

fn run_command(cwd: &Path, program: &str, args: &[&str], error_message: &str) -> Result<()> {
    run_command_with_env(cwd, program, args, &[], error_message)
}

pub(crate) fn command_output_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    // Run the node runner in its own process group so a timeout can kill the
    // whole tree (node + the Playwright/Chromium children it spawns), not just
    // the node parent — which would otherwise leave an orphaned browser.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().context("failed to launch command")?;
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return child
                .wait_with_output()
                .context("failed to collect command output");
        }
        if Instant::now() >= deadline {
            kill_process_tree(&mut child);
            let _ = child.wait();
            anyhow::bail!("timed out after {}ms", timeout.as_millis());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Kill a spawned child and (on unix) its whole process group, so descendant
/// processes such as Chromium are not orphaned when a runner times out.
fn kill_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // `process_group(0)` made the child a group leader (pgid == child pid),
        // so a negative pid signals every process in that group.
        let pid = child.id() as libc::pid_t;
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

fn run_command_with_env(
    cwd: &Path,
    program: &str,
    args: &[&str],
    envs: &[(&str, &Path)],
    error_message: &str,
) -> Result<()> {
    let resolved_program = find_command_on_path(program)
        .with_context(|| format!("{error_message}: `{program}` was not found on PATH"))?;
    let mut command = Command::new(&resolved_program);
    command.current_dir(cwd).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output().with_context(|| {
        format!(
            "{error_message}: failed to launch `{}`",
            resolved_program.display()
        )
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("exit status {}", output.status)
    };
    anyhow::bail!("{error_message}: {detail}");
}

fn detect_tool(program: &str, version_args: &[&str]) -> ToolStatus {
    let path = find_command_on_path(program);
    let Some(path) = path else {
        return ToolStatus {
            available: false,
            path: None,
            version: None,
        };
    };
    let version = Command::new(&path)
        .args(version_args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if !stdout.is_empty() {
                stdout
            } else {
                stderr
            }
        })
        .filter(|value| !value.is_empty());
    ToolStatus {
        available: true,
        path: Some(path.display().to_string()),
        version,
    }
}

fn parse_node_major_version(version: &str) -> Option<u64> {
    version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|value| value.parse::<u64>().ok())
}

fn ensure_node_runtime_compatible() -> Result<()> {
    let node = detect_tool("node", &["--version"]);
    let Some(version) = node.version.as_deref() else {
        anyhow::bail!(
            "node is required for CTOX browser automation and browser reference installation"
        );
    };
    let Some(major) = parse_node_major_version(version) else {
        anyhow::bail!(
            "failed to parse node version `{version}`; browser automation requires Node >= {MINIMUM_NODE_MAJOR}"
        );
    };
    if major < MINIMUM_NODE_MAJOR {
        anyhow::bail!(
            "node {version} is too old for CTOX browser automation; require Node >= {MINIMUM_NODE_MAJOR}"
        );
    }
    Ok(())
}

pub(crate) fn find_command_on_path(program: &str) -> Option<PathBuf> {
    let command_names = command_file_names(program);
    if program.contains('/') || program.contains('\\') {
        return command_names
            .into_iter()
            .map(PathBuf::from)
            .find(|candidate| candidate.is_file());
    }
    let path_env = std::env::var_os("PATH")?;
    std::env::split_paths(&path_env)
        .flat_map(|dir| command_names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

fn command_file_names(program: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        if Path::new(program).extension().is_some() {
            return vec![program.to_string()];
        }
        let mut names = Vec::new();
        let path_extensions =
            std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
        names.extend(
            path_extensions
                .split(';')
                .map(str::trim)
                .filter(|extension| !extension.is_empty())
                .map(|extension| format!("{program}{extension}")),
        );
        // Git-for-Windows also ships extensionless POSIX shims such as `npm`.
        // Prefer native PATHEXT launchers (`npm.cmd`) before that fallback.
        names.push(program.to_string());
        names
    }
    #[cfg(not(windows))]
    vec![program.to_string()]
}

fn resolve_reference_dir(root: &Path, args: &[String]) -> PathBuf {
    browser_reference_dir(root, find_flag_value(args, "--dir").map(PathBuf::from))
}

fn browser_reference_dir(root: &Path, dir: Option<PathBuf>) -> PathBuf {
    // Precedence: explicit `--dir`/request value, then the SQLite runtime config
    // key, then the default. Runtime config lives in the CTOX SQLite store (not
    // a process-env toggle) per the repository guardrails.
    let configured = dir
        .or_else(|| {
            crate::runtime_config::get(root, "CTOX_WEB_BROWSER_REFERENCE_DIR").map(PathBuf::from)
        })
        .unwrap_or_else(|| PathBuf::from(DEFAULT_REFERENCE_RELATIVE_DIR));
    resolve_root_relative_path(root, configured)
}

fn bootstrap_payload(reference_dir: &Path) -> serde_json::Value {
    let chromium_fallback_executable = find_browser_executable(reference_dir);
    let chromium_fallback_string = chromium_fallback_executable
        .as_ref()
        .map(|value| value.display().to_string());
    json!({
        "ok": true,
        "reference_dir": reference_dir,
        "chromium_fallback_executable": chromium_fallback_string,
        "snippet": bootstrap_snippet(chromium_fallback_string.as_deref()),
    })
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn bootstrap_snippet(chromium_fallback_executable: Option<&str>) -> String {
    let launch_options = if let Some(path) = chromium_fallback_executable {
        format!(
            "{{ headless: true, executablePath: \"{}\" }}",
            path.replace('\\', "\\\\").replace('"', "\\\"")
        )
    } else {
        "{ headless: true }".to_string()
    };
    format!(
        "const {{ chromium }} = await import(\"patchright\");\nconst browser = await chromium.launch({launch_options});\nconst context = await browser.newContext();\nconst page = await context.newPage();\nawait page.goto(\"http://127.0.0.1:3000\", {{ waitUntil: \"domcontentloaded\" }});\nconsole.log(await page.title());\nawait browser.close();"
    )
}

fn find_browser_executable(reference_dir: &Path) -> Option<PathBuf> {
    find_playwright_chromium_executable_in(&playwright_browser_cache_dir(reference_dir))
}

fn find_capture_browser_executable(reference_dir: &Path) -> Option<(&'static str, PathBuf)> {
    select_capture_browser_executable(find_browser_executable(reference_dir))
}

fn select_capture_browser_executable(
    playwright_cache: Option<PathBuf>,
) -> Option<(&'static str, PathBuf)> {
    playwright_cache.map(|path| ("playwright-cache", path))
}

fn resolve_root_relative_path(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

pub(crate) fn playwright_browser_cache_dir(reference_dir: &Path) -> PathBuf {
    reference_dir.join(LOCAL_PLAYWRIGHT_BROWSERS_RELATIVE_DIR)
}

fn find_playwright_chromium_executable_in(cache_root: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(&cache_root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.starts_with("chromium-") || name.contains("headless") {
            continue;
        }
        for relative in [
            "chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            "chrome-mac/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            "chrome-linux64/chrome",
            "chrome-linux/chrome",
            "chrome-win64/chrome.exe",
            "chrome-win/chrome.exe",
        ] {
            let candidate = path.join(relative);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn print_json(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn parse_browser_automation_source(raw: &str) -> Result<(BrowserAutomationDirective, String)> {
    let normalized = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let mut directive = BrowserAutomationDirective::default();
    let mut lines = normalized.lines();
    let mut body = normalized.to_string();

    if let Some(first_line) = lines.next() {
        let trimmed = first_line.trim();
        if let Some(rest) = trimmed.strip_prefix("// ctox-browser:") {
            directive = parse_browser_automation_directive(rest)?;
            body = lines.collect::<Vec<_>>().join("\n");
        }
    }

    let body = body.trim().to_string();
    if body.is_empty() {
        anyhow::bail!("browser automation source must not be empty");
    }
    Ok((directive, body))
}

fn parse_browser_automation_directive(raw: &str) -> Result<BrowserAutomationDirective> {
    let mut directive = BrowserAutomationDirective::default();
    for token in raw.split([',', ' ', '\t']) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        match key.trim() {
            "timeout_ms" | "timeout" => {
                directive.timeout_ms = Some(
                    value
                        .trim()
                        .parse::<u64>()
                        .with_context(|| format!("failed to parse browser automation {key}"))?,
                );
            }
            _ => {}
        }
    }
    Ok(directive)
}

fn wait_for_devtools_url(profile_dir: &Path, timeout: Duration) -> Result<String> {
    let port_file = profile_dir.join("DevToolsActivePort");
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(raw) = fs::read_to_string(&port_file) {
            let mut lines = raw.lines();
            let Some(port) = lines
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                anyhow::bail!("Chrome DevToolsActivePort did not contain a port");
            };
            return Ok(format!("http://127.0.0.1:{port}"));
        }
        if Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for Chrome DevToolsActivePort in {}",
                profile_dir.display()
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn build_browser_runner_script(
    source: &str,
    timeout_ms: u64,
    fallback_executable: Option<&str>,
) -> Result<String> {
    let encoded_source =
        serde_json::to_string(source).context("failed to encode browser automation source")?;
    let encoded_executable = serde_json::to_string(&fallback_executable)
        .context("failed to encode browser executable override")?;
    Ok(format!(
        r#"import process from "node:process";
import path from "node:path";
import util from "node:util";

const timeoutMs = {timeout_ms};
const userSource = {encoded_source};
const fallbackExecutable = {encoded_executable};
const logs = [];

const formatLogValue = (value) =>
  typeof value === "string"
    ? value
    : util.inspect(value, {{ depth: 4, breakLength: Infinity, maxArrayLength: 32 }});

for (const level of ["log", "info", "warn", "debug", "error"]) {{
  console[level] = (...args) => {{
    logs.push({{
      level,
      text: args.map(formatLogValue).join(" "),
    }});
  }};
}}

const safeSerialize = (value, depth = 0, seen = new WeakSet()) => {{
  if (depth > 4) return "[depth limit]";
  if (value === undefined) return null;
  if (value === null || typeof value === "string" || typeof value === "number" || typeof value === "boolean") {{
    return value;
  }}
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "function") return `[Function ${{value.name || "anonymous"}}]`;
  if (Array.isArray(value)) {{
    return value.slice(0, 32).map((item) => safeSerialize(item, depth + 1, seen));
  }}
  if (typeof value === "object") {{
    if (seen.has(value)) return "[circular]";
    seen.add(value);
    if (typeof value.url === "function" && typeof value.title === "function") {{
      return {{
        kind: (value.constructor && value.constructor.name) || "PageLike",
        url: (() => {{ try {{ return value.url(); }} catch {{ return null; }} }})(),
      }};
    }}
    const out = {{}};
    for (const [key, item] of Object.entries(value).slice(0, 32)) {{
      out[key] = safeSerialize(item, depth + 1, seen);
    }}
    return out;
  }}
  return String(value);
}};

const pageMetadata = async (page) => {{
  let url = null;
  let title = null;
  try {{
    url = page && typeof page.url === "function" ? page.url() : null;
  }} catch {{}}
  try {{
    title = page ? await page.title() : null;
  }} catch {{}}
  return {{ url, title }};
}};

const detectionSnapshot = async (page) => {{
  const metadata = await pageMetadata(page);
  let text = "";
  let html = "";
  try {{ text = await page.locator("body").innerText({{ timeout: 1500 }}); }} catch {{}}
  try {{ html = await page.content(); }} catch {{}}
  const textCorpus = `${{metadata.title || ""}} ${{metadata.url || ""}} ${{text.slice(0, 8000)}}`.toLowerCase();
  const htmlCorpus = html.slice(0, 32000).toLowerCase();
  const checks = [
    ["cloudflare_challenge", /just a moment|performing security verification|cf-chl-|challenge-platform/, `${{textCorpus}} ${{htmlCorpus}}`],
    ["turnstile", /cf-turnstile|challenges\.cloudflare\.com\/turnstile/, htmlCorpus],
    ["captcha", /g-recaptcha|h-captcha|recaptcha\/api|hcaptcha\.com\/1\/api|data-sitekey/, htmlCorpus],
    ["human_verification", /verify (that )?you are human|human verification|security check/, textCorpus],
    ["access_denied", /access denied|request blocked/, textCorpus],
  ];
  return {{
    ...metadata,
    markers: checks.filter(([, pattern, corpus]) => pattern.test(corpus)).map(([marker]) => marker),
  }};
}};

const emit = async (payload) => {{
  process.stdout.write(JSON.stringify(payload));
}};

const {{ chromium, firefox, webkit }} = await import("patchright");
const defaultUserAgent = (() => {{
  switch (process.platform) {{
    case "darwin":
      return "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";
    case "win32":
      return "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";
    default:
      return "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";
  }}
}})();
// Derive locale from OS so Page (CDP-overridden) and Service/Web Worker
// (which read OS locale) stay consistent. Anti-bots flag the mismatch.
// process.env.LANG / LC_ALL on Unix carries the OS locale. Empty env
// (eg. shells without it) — leave locale unset so Chromium and Worker
// both pick the OS default and stay aligned.
const hostLocale = (() => {{
  const raw = process.env.LC_ALL || process.env.LC_MESSAGES || process.env.LANG || "";
  const stripped = raw.split(".")[0];
  if (stripped && stripped.length >= 2) {{
    return stripped.replace("_", "-");
  }}
  return null;
}})();
const launchArgs = [
  `--user-agent=${{defaultUserAgent}}`,
  "--disable-background-networking",
  "--disable-component-update",
  "--disable-default-apps",
  "--disable-sync",
  "--metrics-recording-only",
  "--no-first-run",
  "--no-service-autorun",
  "--remote-allow-origins=",
];
if (hostLocale) launchArgs.push(`--lang=${{hostLocale}}`);
const launchOptions = {{
  headless: true,
  ignoreDefaultArgs: ["--enable-automation", "--enable-unsafe-swiftshader"],
  args: launchArgs,
}};
if (fallbackExecutable) {{
  launchOptions.executablePath = fallbackExecutable;
}}
const defaultClientHints = (() => {{
  let platform = '"Linux"';
  if (process.platform === "darwin") platform = '"macOS"';
  else if (process.platform === "win32") platform = '"Windows"';
  return {{
    "Sec-CH-UA": '"Chromium";v="146", "Google Chrome";v="146", "Not.A/Brand";v="24"',
    "Sec-CH-UA-Mobile": "?0",
    "Sec-CH-UA-Platform": platform,
  }};
}})();
const contextOptions = {{
  viewport: {{ width: 1920, height: 947 }},
  userAgent: defaultUserAgent,
  extraHTTPHeaders: defaultClientHints,
}};
if (hostLocale) contextOptions.locale = hostLocale;

const profileDir = path.join(process.cwd(), ".ctox-browser-profile");
const browser = await chromium.launch(launchOptions);
const context = await browser.newContext(contextOptions);
try {{
  await context.addInitScript({{ path: path.join(process.cwd(), "stealth_init.js") }});
}} catch (err) {{
  logs.push({{ level: "warn", text: `stealth_init.js skipped: ${{err && err.message ? err.message : err}}` }});
}}
const page = await context.newPage();
const humanlike = await import("./humanlike.mjs").catch(() => null);
globalThis.chromium = chromium;
globalThis.firefox = firefox;
globalThis.webkit = webkit;
globalThis.context = context;
globalThis.page = page;
globalThis.browser = browser;
globalThis.humanlike = humanlike;
	const ctoxBrowserApi = {{
	  logs,
	  profileDir,
	  locatorFor(target) {{
	    if (typeof target === "string") return page.locator(target);
	    if (!target || typeof target !== "object") throw new Error("ctoxBrowser target must be a selector string or target object");
	    if (target.selector) return page.locator(target.selector);
	    if (target.testId) return page.getByTestId(String(target.testId));
	    if (target.role && target.name) return page.getByRole(String(target.role), {{ name: String(target.name), exact: true }});
	    if (target.label) return page.getByLabel(String(target.label), {{ exact: true }});
	    if (target.placeholder) return page.getByPlaceholder(String(target.placeholder), {{ exact: true }});
	    if (target.text) return page.getByText(String(target.text), {{ exact: true }});
	    throw new Error("ctoxBrowser target has no usable selector, testId, role/name, label, placeholder, or text");
	  }},
	  async resolveTarget(target) {{
	    const locator = this.locatorFor(target);
	    const count = await locator.count();
	    if (count !== 1) {{
	      throw new Error(`ctoxBrowser target resolved to ${{count}} elements; refine the target before acting`);
	    }}
	    return locator;
	  }},
	  async observe(options = {{}}) {{
	    const limit = Number.isFinite(options.limit) ? Math.max(1, Math.min(200, Math.floor(options.limit))) : 80;
	    const textMax = Number.isFinite(options.textMax) ? Math.max(20, Math.min(400, Math.floor(options.textMax))) : 120;
	    const dom = await page.evaluate(
	      ({{ limit, textMax }}) => {{
	        const trim = (value, max = textMax) => {{
	          const text = String(value ?? "").replace(/\s+/g, " ").trim();
	          return text.length > max ? text.slice(0, max - 1) + "..." : text;
	        }};
	        const cssEscape = (value) => globalThis.CSS && typeof globalThis.CSS.escape === "function"
	          ? globalThis.CSS.escape(String(value))
	          : String(value).replace(/["\\]/g, "\\$&");
	        const visible = (element) => {{
	          const style = globalThis.getComputedStyle(element);
	          const box = element.getBoundingClientRect();
	          return style.visibility !== "hidden"
	            && style.display !== "none"
	            && Number(style.opacity || "1") > 0
	            && box.width > 0
	            && box.height > 0;
	        }};
	        const textOf = (element) => trim(
	          element.getAttribute("aria-label")
	          || element.getAttribute("title")
	          || element.getAttribute("alt")
	          || element.getAttribute("placeholder")
	          || element.value
	          || element.innerText
	          || element.textContent
	          || ""
	        );
	        const candidatesFor = (element) => {{
	          const candidates = [];
	          const testId = element.getAttribute("data-testid");
	          if (testId) candidates.push(`[data-testid="${{cssEscape(testId)}}"]`);
	          for (const attr of element.getAttributeNames()) {{
	            if (attr.startsWith("data-") && attr !== "data-testid") {{
	              const value = element.getAttribute(attr);
	              if (value && value.length <= 80) candidates.push(`[${{attr}}="${{cssEscape(value)}}"]`);
	            }}
	          }}
	          const id = element.getAttribute("id");
	          if (id) candidates.push(`#${{cssEscape(id)}}`);
	          const href = element.getAttribute("href");
	          if (href) candidates.push(`${{element.tagName.toLowerCase()}}[href="${{cssEscape(href)}}"]`);
	          const name = element.getAttribute("name");
	          if (name) candidates.push(`${{element.tagName.toLowerCase()}}[name="${{cssEscape(name)}}"]`);
	          return [...new Set(candidates)].slice(0, 6);
	        }};
	        const selector = [
	          "a",
	          "button",
	          "input",
	          "textarea",
	          "select",
	          "summary",
	          "[role]",
	          "[data-testid]",
	          "[onclick]",
	          "[contenteditable='true']",
	        ].join(",");
	        const targets = [];
	        for (const element of Array.from(document.querySelectorAll(selector))) {{
	          if (!visible(element)) continue;
	          const box = element.getBoundingClientRect();
	          const candidates = candidatesFor(element);
	          targets.push({{
	            id: `target-${{targets.length + 1}}`,
	            tag: element.tagName.toLowerCase(),
	            role: element.getAttribute("role") || null,
	            name: element.getAttribute("aria-label") || textOf(element) || null,
	            text: textOf(element) || null,
	            testId: element.getAttribute("data-testid") || null,
	            href: element.getAttribute("href") || null,
	            selector: candidates[0] || null,
	            candidates,
	            box: {{
	              x: Math.round(box.x),
	              y: Math.round(box.y),
	              width: Math.round(box.width),
	              height: Math.round(box.height),
	            }},
	          }});
	          if (targets.length >= limit) break;
	        }}
	        return {{
	          documentText: trim(document.body ? document.body.innerText : "", Math.max(textMax * 8, 800)),
	          targets,
	        }};
	      }},
	      {{ limit, textMax }}
	    );
	    return {{
	      url: page.url(),
	      title: await page.title(),
	      documentText: dom.documentText,
	      targets: dom.targets,
	    }};
	  }},
	  async goto(url, options = {{}}) {{
	    await page.goto(url, {{
	      waitUntil: options.waitUntil || "domcontentloaded",
	      timeout: options.timeoutMs || 30_000,
	    }});
	    return await this.observe(options);
	  }},
	  async click(target, options = {{}}) {{
	    const locator = await this.resolveTarget(target);
	    await locator.click(options);
	    return await this.observe(options);
	  }},
	  async fill(target, value, options = {{}}) {{
	    const locator = await this.resolveTarget(target);
	    await locator.fill(String(value), options);
	    return await this.observe(options);
	  }},
	  async press(target, key, options = {{}}) {{
	    const locator = await this.resolveTarget(target);
	    await locator.press(String(key), options);
	    return await this.observe(options);
	  }},
	  async screenshot(options = {{}}) {{
	    const buffer = await page.screenshot({{ fullPage: !!options.fullPage }});
	    return {{ mimeType: "image/png", base64: buffer.toString("base64") }};
	  }},
	  async logsFor(levels = ["error", "warning", "warn"]) {{
	    const wanted = new Set(levels);
	    return logs.filter((entry) => wanted.has(entry.level));
	  }},
	}};
	globalThis.ctoxBrowser = ctoxBrowserApi;
let timeoutHandle = null;

try {{
  const AsyncFunction = Object.getPrototypeOf(async function () {{}}).constructor;
  const userFunction = new AsyncFunction(userSource);
  const result = await Promise.race([
    userFunction(),
    new Promise((_, reject) =>
      timeoutHandle = setTimeout(
        () => reject(new Error(`browser automation timed out after ${{timeoutMs}}ms`)),
        timeoutMs
      )
    ),
  ]);
  if (timeoutHandle) {{
    clearTimeout(timeoutHandle);
    timeoutHandle = null;
  }}
  await emit({{
    ok: true,
    tool: "ctox_browser_automation",
    result: safeSerialize(result),
    logs,
    page: await pageMetadata(page),
    detection: await detectionSnapshot(page),
  }});
}} catch (error) {{
  if (timeoutHandle) {{
    clearTimeout(timeoutHandle);
    timeoutHandle = null;
  }}
  await emit({{
    ok: false,
    tool: "ctox_browser_automation",
    error: (error && error.stack) || String(error),
    logs,
    page: await pageMetadata(page),
    detection: await detectionSnapshot(page),
  }});
}} finally {{
  try {{
    await context.close();
  }} catch {{
    // ignore close errors; the JSON payload is already emitted
  }}
  try {{
    await browser.close();
  }} catch {{
    // ignore close errors; the JSON payload is already emitted
  }}
}}
"#
    ))
}

/// Spawn parameters for a long-lived (persistent) Chromium runtime.
#[derive(Debug, Clone, Default)]
pub struct PersistentBrowserSpawn {
    pub dir: Option<PathBuf>,
    pub viewport_w: u64,
    pub viewport_h: u64,
    pub profile_dir: Option<PathBuf>,
    pub private_profile: bool,
    pub egress_allow_hosts: Vec<String>,
    pub downloads_dir: Option<PathBuf>,
}

/// A long-lived Chromium/Patchright process driven over newline-delimited JSON.
///
/// Each request is a single JSON line on stdin; each response is a single JSON
/// line on stdout. Logs are routed to stderr so stdout stays a clean response
/// channel. This is the persistent counterpart to [`run_browser_automation`],
/// which spawns a fresh one-shot process per call.
pub struct PersistentBrowserHandle {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    runner_path: PathBuf,
    profile_dir: Option<PathBuf>,
    downloads_dir: Option<PathBuf>,
    remove_profile_on_close: bool,
}

impl PersistentBrowserHandle {
    fn wait_until_ready(&mut self, timeout: Duration) -> Result<()> {
        let pid = self.child.id();
        let timed_out = Arc::new(AtomicBool::new(false));
        let watchdog_timed_out = Arc::clone(&timed_out);
        let (cancel_tx, cancel_rx) = mpsc::channel::<()>();
        let watchdog = thread::spawn(move || {
            if cancel_rx.recv_timeout(timeout).is_err() {
                watchdog_timed_out.store(true, Ordering::Release);
                terminate_persistent_browser_process_tree(pid);
            }
        });

        let readiness = (|| -> Result<()> {
            loop {
                let mut buf = String::new();
                let read = self
                    .stdout
                    .read_line(&mut buf)
                    .context("failed to read persistent browser readiness")?;
                if read == 0 {
                    anyhow::bail!("persistent browser runtime exited before becoming ready");
                }
                let trimmed = buf.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                    if value.get("ready").and_then(Value::as_bool) == Some(true) {
                        return Ok(());
                    }
                    if value.get("ok").and_then(Value::as_bool) == Some(false) {
                        let detail = value
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("persistent browser runtime failed to start");
                        anyhow::bail!("{detail}");
                    }
                }
            }
        })();

        let _ = cancel_tx.send(());
        let _ = watchdog.join();
        if timed_out.load(Ordering::Acquire) {
            anyhow::bail!(
                "persistent browser runtime did not become ready after {}ms",
                timeout.as_millis()
            );
        }
        readiness
    }

    /// Send one operation and block until the matching response line arrives.
    pub fn request(&mut self, op: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        let mut message = match params {
            Value::Object(map) => Value::Object(map),
            Value::Null => Value::Object(serde_json::Map::new()),
            other => {
                let mut map = serde_json::Map::new();
                map.insert("value".to_string(), other);
                Value::Object(map)
            }
        };
        if let Some(obj) = message.as_object_mut() {
            obj.insert("id".to_string(), Value::from(id));
            obj.insert("op".to_string(), Value::String(op.to_string()));
        }
        let line = serde_json::to_string(&message)
            .context("failed to encode persistent browser request")?;
        self.stdin
            .write_all(line.as_bytes())
            .context("failed to write persistent browser request")?;
        self.stdin
            .write_all(b"\n")
            .context("failed to write persistent browser request newline")?;
        self.stdin
            .flush()
            .context("failed to flush persistent browser request")?;
        loop {
            let mut buf = String::new();
            let read = self
                .stdout
                .read_line(&mut buf)
                .context("failed to read persistent browser response")?;
            if read == 0 {
                anyhow::bail!("persistent browser runtime closed stdout before responding");
            }
            let trimmed = buf.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return Ok(value);
            }
        }
    }

    /// Send one operation with a native deadline. If the JavaScript runtime
    /// stops responding, terminate its isolated process tree so the blocking
    /// stdout read is released and the profile can be reopened cleanly.
    pub fn request_with_timeout(
        &mut self,
        op: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value> {
        let pid = self.child.id();
        let timed_out = Arc::new(AtomicBool::new(false));
        let watchdog_timed_out = Arc::clone(&timed_out);
        let (cancel_tx, cancel_rx) = mpsc::channel::<()>();
        let watchdog = thread::spawn(move || {
            if cancel_rx.recv_timeout(timeout).is_err() {
                watchdog_timed_out.store(true, Ordering::Release);
                terminate_persistent_browser_process_tree(pid);
            }
        });

        let result = self.request(op, params);
        let _ = cancel_tx.send(());
        let _ = watchdog.join();
        if timed_out.load(Ordering::Acquire) {
            anyhow::bail!(
                "persistent browser request `{op}` timed out after {}ms",
                timeout.as_millis()
            );
        }
        result
    }

    /// Ask the runtime to close gracefully, then ensure the process is gone and
    /// the generated runner file is cleaned up. Never panics; best effort.
    pub fn shutdown(&mut self) {
        let _ = self.request_with_timeout("close", json!({}), Duration::from_secs(5));
        terminate_persistent_browser_process_tree(self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.runner_path);
        if self.remove_profile_on_close {
            if let Some(profile_dir) = &self.profile_dir {
                let _ = fs::remove_dir_all(profile_dir);
            }
            if let Some(downloads_dir) = &self.downloads_dir {
                let _ = fs::remove_dir_all(downloads_dir);
            }
        }
    }
}

#[cfg(unix)]
fn terminate_persistent_browser_process_tree(pid: u32) {
    // The runtime is spawned into a dedicated process group. A negative PID
    // targets that group, including Chromium children holding the profile.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

#[cfg(windows)]
fn terminate_persistent_browser_process_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(any(unix, windows)))]
fn terminate_persistent_browser_process_tree(_pid: u32) {}

impl Drop for PersistentBrowserHandle {
    fn drop(&mut self) {
        terminate_persistent_browser_process_tree(self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.runner_path);
        if self.remove_profile_on_close {
            if let Some(profile_dir) = &self.profile_dir {
                let _ = fs::remove_dir_all(profile_dir);
            }
            if let Some(downloads_dir) = &self.downloads_dir {
                let _ = fs::remove_dir_all(downloads_dir);
            }
        }
    }
}

/// Launch a persistent Chromium runtime and wait for it to signal readiness.
pub fn spawn_persistent_browser(
    root: &Path,
    spawn: &PersistentBrowserSpawn,
) -> Result<PersistentBrowserHandle> {
    let reference_dir = browser_reference_dir(root, spawn.dir.clone());
    fs::create_dir_all(&reference_dir).with_context(|| {
        format!(
            "failed to create browser automation reference dir {}",
            reference_dir.display()
        )
    })?;
    let _ = ensure_reference_package_json(&reference_dir)?;
    ensure_humanlike_module(&reference_dir)?;
    ensure_stealth_init_module(&reference_dir)?;
    let _doctor = ensure_browser_automation_ready(&reference_dir, "persistent browser automation")?;
    let Some(node_path) = find_command_on_path("node") else {
        anyhow::bail!("node is required for browser automation");
    };

    let viewport_w = spawn.viewport_w.clamp(320, 3840);
    let viewport_h = spawn.viewport_h.clamp(240, 2160);
    let fallback_executable =
        find_browser_executable(&reference_dir).map(|value| value.display().to_string());
    let profile_dir = spawn.profile_dir.clone().unwrap_or_else(|| {
        reference_dir.join(format!(
            ".ctox-browser-private-{}-{}",
            std::process::id(),
            unix_ts()
        ))
    });
    fs::create_dir_all(&profile_dir)
        .with_context(|| format!("failed to create browser profile {}", profile_dir.display()))?;
    cleanup_stale_chromium_profile_locks(&profile_dir)?;
    let downloads_dir = spawn
        .downloads_dir
        .clone()
        .unwrap_or_else(|| profile_dir.join("Downloads"));
    fs::create_dir_all(&downloads_dir).with_context(|| {
        format!(
            "failed to create browser downloads directory {}",
            downloads_dir.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&profile_dir, fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(&downloads_dir, fs::Permissions::from_mode(0o700))?;
    }
    let runner_path = reference_dir.join(format!(
        ".ctox-browser-live-{}-{}.mjs",
        std::process::id(),
        unix_ts()
    ));
    let runner_source = build_persistent_browser_runner_script(
        viewport_w,
        viewport_h,
        fallback_executable.as_deref(),
        &profile_dir,
        &downloads_dir,
        &spawn.egress_allow_hosts,
    )?;
    fs::write(&runner_path, runner_source)
        .with_context(|| format!("failed to write {}", runner_path.display()))?;

    let mut command = Command::new(&node_path);
    command
        .current_dir(&reference_dir)
        .env(
            "PLAYWRIGHT_BROWSERS_PATH",
            playwright_browser_cache_dir(&reference_dir),
        )
        .arg(&runner_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            let _ = fs::remove_file(&runner_path);
            return Err(err).with_context(|| {
                format!(
                    "failed to launch persistent browser runtime with {}",
                    node_path.display()
                )
            });
        }
    };
    let stdin = child
        .stdin
        .take()
        .context("persistent browser runtime is missing stdin")?;
    let stdout = child
        .stdout
        .take()
        .context("persistent browser runtime is missing stdout")?;
    let mut handle = PersistentBrowserHandle {
        child,
        stdin,
        stdout: BufReader::new(stdout),
        next_id: 0,
        runner_path,
        profile_dir: Some(profile_dir),
        downloads_dir: Some(downloads_dir),
        remove_profile_on_close: spawn.private_profile || spawn.profile_dir.is_none(),
    };

    // Chromium profile locks and browser startup can stall before any request
    // is sent, so readiness needs the same process-level protection as calls.
    handle.wait_until_ready(Duration::from_secs(45))?;
    Ok(handle)
}

#[cfg(unix)]
fn cleanup_stale_chromium_profile_locks(profile_dir: &Path) -> Result<()> {
    let singleton_lock = profile_dir.join("SingletonLock");
    if !singleton_lock.exists() && fs::symlink_metadata(&singleton_lock).is_err() {
        return Ok(());
    }
    let Ok(target) = fs::read_link(&singleton_lock) else {
        return Ok(());
    };
    let owner_pid = target
        .file_name()
        .and_then(|value| value.to_str())
        .and_then(|value| value.rsplit('-').next())
        .and_then(|value| value.parse::<i32>().ok());
    let Some(owner_pid) = owner_pid else {
        return Ok(());
    };
    let owner_alive = unsafe {
        if libc::kill(owner_pid, 0) == 0 {
            true
        } else {
            std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
    };
    if owner_alive {
        return Ok(());
    }
    for name in ["SingletonLock", "SingletonCookie", "SingletonSocket"] {
        let path = profile_dir.join(name);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to remove stale browser profile lock {}",
                        path.display()
                    )
                });
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn cleanup_stale_chromium_profile_locks(_profile_dir: &Path) -> Result<()> {
    Ok(())
}

fn build_persistent_browser_runner_script(
    viewport_w: u64,
    viewport_h: u64,
    fallback_executable: Option<&str>,
    profile_dir: &Path,
    downloads_dir: &Path,
    egress_allow_hosts: &[String],
) -> Result<String> {
    let encoded_executable = serde_json::to_string(&fallback_executable)
        .context("failed to encode browser executable override")?;
    let encoded_profile_dir = serde_json::to_string(&profile_dir.display().to_string())
        .context("failed to encode browser profile directory")?;
    let encoded_downloads_dir = serde_json::to_string(&downloads_dir.display().to_string())
        .context("failed to encode browser downloads directory")?;
    let encoded_allow_hosts = serde_json::to_string(egress_allow_hosts)
        .context("failed to encode browser egress allow-list")?;
    Ok(format!(
        r#"import process from "node:process";
	import path from "node:path";
	import dns from "node:dns/promises";
	import readline from "node:readline";
	import util from "node:util";

	const VIEWPORT_W = {viewport_w};
	const VIEWPORT_H = {viewport_h};
	const fallbackExecutable = {encoded_executable};
	const profileDir = {encoded_profile_dir};
	const downloadsDir = {encoded_downloads_dir};
	const egressAllowHosts = new Set({encoded_allow_hosts}.map((value) => String(value).toLowerCase()));
	const logs = [];

	const formatLogValue = (value) =>
	  typeof value === "string"
	    ? value
	    : util.inspect(value, {{ depth: 4, breakLength: Infinity, maxArrayLength: 32 }});

	for (const level of ["log", "info", "warn", "debug", "error"]) {{
	  console[level] = (...args) => {{
	    try {{
	      const text = args.map(formatLogValue).join(" ");
	      logs.push({{ level, text }});
	      process.stderr.write(`[live] ${{text}}\n`);
	    }} catch {{}}
	  }};
	}}

	const respond = (payload) => {{
	  process.stdout.write(JSON.stringify(payload) + "\n");
	}};

	const safeSerialize = (value, depth = 0, seen = new WeakSet()) => {{
	  if (depth > 4) return "[depth limit]";
	  if (value === undefined) return null;
	  if (value === null || typeof value === "string" || typeof value === "number" || typeof value === "boolean") {{
	    return value;
	  }}
	  if (typeof value === "bigint") return value.toString();
	  if (typeof value === "function") return `[Function ${{value.name || "anonymous"}}]`;
	  if (Array.isArray(value)) {{
	    return value.slice(0, 64).map((item) => safeSerialize(item, depth + 1, seen));
	  }}
	  if (typeof value === "object") {{
	    if (seen.has(value)) return "[circular]";
	    seen.add(value);
	    if (typeof value.url === "function" && typeof value.title === "function") {{
	      return {{
	        kind: (value.constructor && value.constructor.name) || "PageLike",
	        url: (() => {{ try {{ return value.url(); }} catch {{ return null; }} }})(),
	      }};
	    }}
	    const out = {{}};
	    for (const [key, item] of Object.entries(value).slice(0, 64)) {{
	      out[key] = safeSerialize(item, depth + 1, seen);
	    }}
	    return out;
	  }}
	  return String(value);
	}};

	const pageMetadata = async () => {{
	  let url = null;
	  let title = null;
	  try {{ url = page && typeof page.url === "function" ? page.url() : null; }} catch {{}}
	  try {{ title = page ? await page.title() : null; }} catch {{}}
	  return {{ url, title }};
	}};

const {{ chromium }} = await import("patchright");
const defaultUserAgent = (() => {{
  switch (process.platform) {{
    case "darwin":
      return "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";
    case "win32":
      return "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";
    default:
      return "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36";
  }}
}})();
const hostLocale = (() => {{
  const raw = process.env.LC_ALL || process.env.LC_MESSAGES || process.env.LANG || "";
  const stripped = raw.split(".")[0];
  if (stripped && stripped.length >= 2) {{
    return stripped.replace("_", "-");
  }}
  return null;
}})();
const launchArgs = [`--user-agent=${{defaultUserAgent}}`];
if (hostLocale) launchArgs.push(`--lang=${{hostLocale}}`);
const launchOptions = {{
  headless: true,
  ignoreDefaultArgs: ["--enable-automation", "--enable-unsafe-swiftshader"],
  args: launchArgs,
}};
if (fallbackExecutable) {{
  launchOptions.executablePath = fallbackExecutable;
}}
const defaultClientHints = (() => {{
  let platform = '"Linux"';
  if (process.platform === "darwin") platform = '"macOS"';
  else if (process.platform === "win32") platform = '"Windows"';
  return {{
    "Sec-CH-UA": '"Chromium";v="146", "Google Chrome";v="146", "Not.A/Brand";v="24"',
    "Sec-CH-UA-Mobile": "?0",
    "Sec-CH-UA-Platform": platform,
  }};
}})();
const contextOptions = {{
  viewport: {{ width: VIEWPORT_W, height: VIEWPORT_H }},
  userAgent: defaultUserAgent,
  extraHTTPHeaders: defaultClientHints,
  acceptDownloads: true,
}};
if (hostLocale) contextOptions.locale = hostLocale;

let context;
let page;
try {{
  context = await chromium.launchPersistentContext(profileDir, {{ ...launchOptions, ...contextOptions }});
  try {{
    await context.addInitScript({{ path: path.join(process.cwd(), "stealth_init.js") }});
  }} catch {{}}
  page = context.pages()[0] || await context.newPage();
}} catch (error) {{
  respond({{ ok: false, error: (error && error.message) || String(error) }});
  process.exit(1);
}}

// Simple linear navigation model so the UI can enable/disable back/forward.
let historyPos = -1;
let historyMax = -1;
let tabCounter = 0;
const tabIds = new WeakMap();
const ensureTabId = (candidate, preferred = null) => {{
  if (!tabIds.has(candidate)) tabIds.set(candidate, preferred || `tab-${{++tabCounter}}`);
  return tabIds.get(candidate);
}};
ensureTabId(page, "browser_tab_default");
context.on("page", (candidate) => ensureTabId(candidate));
let pendingDialog = null;
let pendingWebAuthn = null;
let pendingHttpAuth = null;
let pendingPermission = null;
const webAuthnSessions = new WeakMap();
const ensureWebAuthn = async (candidate) => {{
  if (webAuthnSessions.has(candidate)) return webAuthnSessions.get(candidate);
  const client = await context.newCDPSession(candidate);
  await client.send("WebAuthn.enable");
  const created = await client.send("WebAuthn.addVirtualAuthenticator", {{ options: {{
    protocol: "ctap2",
    transport: "internal",
    hasResidentKey: true,
    hasUserVerification: true,
    isUserVerified: true,
    automaticPresenceSimulation: false,
  }} }});
  const state = {{ client, authenticatorId: created.authenticatorId }};
  await client.send("Fetch.enable", {{ handleAuthRequests: true }}).catch(() => {{}});
  client.on("Fetch.requestPaused", (event) => {{
    client.send("Fetch.continueRequest", {{ requestId: event.requestId }}).catch(() => {{}});
  }});
  client.on("Fetch.authRequired", (event) => {{
    pendingHttpAuth = {{
      client,
      request_id: event.requestId,
      origin: String(event.request?.url || "").replace(/([?#]).*$/, ""),
      scheme: String(event.authChallenge?.scheme || "basic"),
      realm: String(event.authChallenge?.realm || ""),
    }};
  }});
  webAuthnSessions.set(candidate, state);
  return state;
}};
await context.exposeBinding("__ctoxWebAuthnRequest", async (_source, request) => {{
  pendingWebAuthn = {{
    type: String(request && request.type || "get"),
    rp_id: String(request && request.rp_id || ""),
    requested_at_ms: Date.now(),
  }};
}}).catch(() => {{}});
await context.exposeBinding("__ctoxPermissionRequest", async (source, request) => {{
  pendingPermission = {{
    kind: String(request && request.kind || "unknown"),
    origin: new URL(source.page.url()).origin,
    requested_at_ms: Date.now(),
  }};
}}).catch(() => {{}});
await context.addInitScript(() => {{
  const credentials = navigator.credentials;
  if (!credentials || credentials.__ctoxWrapped) return;
  for (const type of ["create", "get"]) {{
    const original = credentials[type]?.bind(credentials);
    if (!original) continue;
    credentials[type] = (options = {{}}) => {{
      const publicKey = options.publicKey || {{}};
      const rpId = type === "create" ? publicKey.rp?.id : publicKey.rpId;
      globalThis.__ctoxWebAuthnRequest?.({{ type, rp_id: rpId || location.hostname }}).catch?.(() => {{}});
      return original(options);
    }};
  }}
  Object.defineProperty(credentials, "__ctoxWrapped", {{ value: true }});
  const pending = [];
  globalThis.__ctoxResolvePermission = async (kind, accept) => {{
    const index = pending.findIndex((entry) => entry.kind === kind);
    if (index < 0) return false;
    const [entry] = pending.splice(index, 1);
    if (!accept) {{ entry.reject(new DOMException("Permission denied", "NotAllowedError")); return true; }}
    entry.run();
    return true;
  }};
  if (navigator.mediaDevices?.getUserMedia) {{
    const originalGetUserMedia = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
    navigator.mediaDevices.getUserMedia = (constraints = {{}}) => new Promise((resolve, reject) => {{
      const kind = constraints.video ? "camera" : "microphone";
      pending.push({{ kind, reject, run: () => originalGetUserMedia(constraints).then(resolve, reject) }});
      globalThis.__ctoxPermissionRequest?.({{ kind }}).catch?.(() => {{}});
    }});
  }}
  if (globalThis.Notification?.requestPermission) {{
    const originalNotificationPermission = globalThis.Notification.requestPermission.bind(globalThis.Notification);
    globalThis.Notification.requestPermission = () => new Promise((resolve, reject) => {{
      const kind = "notifications";
      pending.push({{ kind, reject, run: () => originalNotificationPermission().then(resolve, reject) }});
      globalThis.__ctoxPermissionRequest?.({{ kind }}).catch?.(() => {{}});
    }});
  }}
  if (navigator.geolocation?.getCurrentPosition) {{
    const originalGeolocation = navigator.geolocation.getCurrentPosition.bind(navigator.geolocation);
    navigator.geolocation.getCurrentPosition = (resolve, reject, options) => {{
      const kind = "geolocation";
      pending.push({{
        kind,
        reject: (error) => reject?.(error),
        run: () => originalGeolocation(resolve, reject, options),
      }});
      globalThis.__ctoxPermissionRequest?.({{ kind }}).catch?.(() => {{}});
    }};
  }}
}}).catch(() => {{}});
await ensureWebAuthn(page).catch(() => {{}});
const downloads = [];
const bindPageEvents = (candidate) => {{
  ensureWebAuthn(candidate).catch(() => {{}});
  candidate.on("dialog", (dialog) => {{ pendingDialog = dialog; }});
  candidate.on("download", async (download) => {{
    const suggested = String(download.suggestedFilename() || "download.bin").replace(/[^a-zA-Z0-9._-]+/g, "_").slice(0, 160);
    const id = `download-${{Date.now()}}-${{downloads.length + 1}}`;
    const target = path.join(downloadsDir, `${{id}}-${{suggested}}`);
    try {{
      await download.saveAs(target);
      downloads.push({{ id, filename: suggested, status: "quarantined", scan_status: "pending" }});
    }} catch (_error) {{
      downloads.push({{ id, filename: suggested, status: "failed", error_code: "browser.download_quarantine_failed" }});
    }}
  }});
}};
context.pages().forEach(bindPageEvents);
context.on("page", bindPageEvents);

const tabState = async () => Promise.all(context.pages().map(async (candidate) => ({{
  id: ensureTabId(candidate),
  url: candidate.url(),
  title: await candidate.title().catch(() => ""),
  active: candidate === page,
}})));

const isBlockedHost = (host) => {{
  const value = String(host || "").toLowerCase().replace(/^\[|\]$/g, "");
  if (value === "localhost" || value === "::1" || value === "0.0.0.0") return true;
  const parts = value.split(".").map(Number);
  if (parts.length === 4 && parts.every((part) => Number.isInteger(part) && part >= 0 && part <= 255)) {{
    return parts[0] === 10 || parts[0] === 127 || parts[0] === 0
      || (parts[0] === 169 && parts[1] === 254)
      || (parts[0] === 172 && parts[1] >= 16 && parts[1] <= 31)
      || (parts[0] === 192 && parts[1] === 168)
      || (parts[0] === 100 && parts[1] >= 64 && parts[1] <= 127);
  }}
  return value.startsWith("fc") || value.startsWith("fd")
    || value.startsWith("fe8") || value.startsWith("fe9")
    || value.startsWith("fea") || value.startsWith("feb");
}};
const assertAllowedUrl = async (raw) => {{
  const parsed = new URL(String(raw));
  if (["data:", "blob:"].includes(parsed.protocol)) return parsed.href;
  if (!["http:", "https:", "about:"].includes(parsed.protocol)) {{
    throw new Error(`blocked browser URL scheme: ${{parsed.protocol}}`);
  }}
  if (parsed.protocol !== "about:" && !egressAllowHosts.has(parsed.hostname.toLowerCase())) {{
    const addresses = await dns.lookup(parsed.hostname, {{ all: true }}).catch(() => []);
    if (isBlockedHost(parsed.hostname) || addresses.length === 0 || addresses.some((entry) => isBlockedHost(entry.address))) {{
      throw new Error(`blocked browser egress host: ${{parsed.hostname}}`);
    }}
  }}
  return parsed.href;
}};
await context.route("**/*", async (route) => {{
  try {{
    await assertAllowedUrl(route.request().url());
    await route.continue();
  }} catch {{
    await route.abort("blockedbyclient");
  }}
}});

const navState = async () => {{
  let url = "about:blank";
  let title = "";
  try {{ url = page.url(); }} catch {{}}
  try {{ title = await page.title(); }} catch {{}}
  return {{
    url,
    title,
    can_go_back: historyPos > 0,
    can_go_forward: historyPos < historyMax,
    active_tab_id: ensureTabId(page),
    tabs: await tabState(),
    dialog: pendingDialog ? {{ type: pendingDialog.type(), message: pendingDialog.message(), default_value: pendingDialog.defaultValue() }} : null,
    downloads: downloads.slice(-20),
    webauthn_request: pendingWebAuthn,
    http_auth_request: pendingHttpAuth ? {{ origin: pendingHttpAuth.origin, scheme: pendingHttpAuth.scheme, realm: pendingHttpAuth.realm }} : null,
    permission_request: pendingPermission,
  }};
}};

const screenshot = async (options = {{}}) => {{
  const format = options.format === "jpeg" ? "jpeg" : "png";
  const buffer = await page.screenshot({{
    fullPage: false,
    type: format,
    ...(format === "jpeg" ? {{ quality: Math.max(40, Math.min(92, Number(options.quality || 82))) }} : {{}}),
    mask: [page.locator('input[type="password"], input[autocomplete="one-time-code"]')],
    maskColor: "black",
  }});
  return {{ mimeType: format === "jpeg" ? "image/jpeg" : "image/png", base64: buffer.toString("base64") }};
}};

const applyInput = async (events) => {{
  let applied = 0;
  for (const event of Array.isArray(events) ? events : []) {{
    try {{
      const type = event.type;
      const x = Number(event.x || 0);
      const y = Number(event.y || 0);
      const button = typeof event.button === "string" && event.button ? event.button : "left";
      if (type === "mouseMove") {{
        await page.mouse.move(x, y);
      }} else if (type === "mouseDown") {{
        await page.mouse.move(x, y);
        await page.mouse.down({{ button }});
      }} else if (type === "mouseUp") {{
        await page.mouse.move(x, y);
        await page.mouse.up({{ button }});
      }} else if (type === "click") {{
        await page.mouse.click(x, y, {{ button }});
      }} else if (type === "wheel") {{
        await page.mouse.move(x, y);
        await page.mouse.wheel(Number(event.dx || 0), Number(event.dy || 0));
      }} else if (type === "keyDown") {{
        if (event.text) {{
          await page.keyboard.type(String(event.text));
        }} else if (event.key) {{
          const modifiers = Array.isArray(event.modifiers) ? event.modifiers : [];
          const key = [...modifiers, String(event.key)].join("+");
          await page.keyboard.press(key);
        }}
      }} else if (type === "keyUp") {{
        // keyDown already performs a full press; ignore the paired keyUp.
      }} else {{
        continue;
      }}
      applied++;
    }} catch {{
      // Skip individual input failures; the batch result still advances.
    }}
  }}
  return applied;
}};

	const observe = async (limit, textMax) => {{
	  const cappedLimit = Number.isFinite(limit) ? Math.max(1, Math.min(80, Math.floor(limit))) : 24;
	  const cappedText = Number.isFinite(textMax) ? Math.max(20, Math.min(4000, Math.floor(textMax))) : 1200;
  let documentText = "";
  try {{
    documentText = await page.evaluate((max) => {{
      const text = String(document.body ? document.body.innerText : "").replace(/\s+/g, " ").trim();
      return text.length > max ? text.slice(0, max - 1) + "..." : text;
    }}, cappedText);
  }} catch {{}}
  let title = "";
	  try {{ title = await page.title(); }} catch {{}}
	  return {{ url: page.url(), title, documentText, target_limit: cappedLimit }};
	}};

	const humanlike = await import("./humanlike.mjs").catch(() => null);
	globalThis.chromium = chromium;
	globalThis.context = context;
	globalThis.page = page;
	globalThis.browser = context.browser();
	globalThis.humanlike = humanlike;
	const ctoxBrowserApi = {{
	  logs,
	  profileDir,
	  locatorFor(target) {{
	    if (typeof target === "string") return page.locator(target);
	    if (!target || typeof target !== "object") throw new Error("ctoxBrowser target must be a selector string or target object");
	    if (target.selector) return page.locator(target.selector);
	    if (target.testId) return page.getByTestId(String(target.testId));
	    if (target.role && target.name) return page.getByRole(String(target.role), {{ name: String(target.name), exact: true }});
	    if (target.label) return page.getByLabel(String(target.label), {{ exact: true }});
	    if (target.placeholder) return page.getByPlaceholder(String(target.placeholder), {{ exact: true }});
	    if (target.text) return page.getByText(String(target.text), {{ exact: true }});
	    throw new Error("ctoxBrowser target has no usable selector, testId, role/name, label, placeholder, or text");
	  }},
	  async resolveTarget(target) {{
	    const locator = this.locatorFor(target);
	    const count = await locator.count();
	    if (count !== 1) {{
	      throw new Error(`ctoxBrowser target resolved to ${{count}} elements; refine the target before acting`);
	    }}
	    return locator;
	  }},
	  async observe(options = {{}}) {{
	    const limit = Number.isFinite(options.limit) ? Math.max(1, Math.min(200, Math.floor(options.limit))) : 80;
	    const textMax = Number.isFinite(options.textMax) ? Math.max(20, Math.min(400, Math.floor(options.textMax))) : 120;
	    const dom = await page.evaluate(
	      ({{ limit, textMax }}) => {{
	        const trim = (value, max = textMax) => {{
	          const text = String(value ?? "").replace(/\s+/g, " ").trim();
	          return text.length > max ? text.slice(0, max - 1) + "..." : text;
	        }};
	        const cssEscape = (value) => globalThis.CSS && typeof globalThis.CSS.escape === "function"
	          ? globalThis.CSS.escape(String(value))
	          : String(value).replace(/["\\]/g, "\\$&");
	        const visible = (element) => {{
	          const style = globalThis.getComputedStyle(element);
	          const box = element.getBoundingClientRect();
	          return style.visibility !== "hidden"
	            && style.display !== "none"
	            && Number(style.opacity || "1") > 0
	            && box.width > 0
	            && box.height > 0;
	        }};
	        const textOf = (element) => trim(
	          element.getAttribute("aria-label")
	          || element.getAttribute("title")
	          || element.getAttribute("alt")
	          || element.getAttribute("placeholder")
	          || element.value
	          || element.innerText
	          || element.textContent
	          || ""
	        );
	        const candidatesFor = (element) => {{
	          const candidates = [];
	          const testId = element.getAttribute("data-testid");
	          if (testId) candidates.push(`[data-testid="${{cssEscape(testId)}}"]`);
	          for (const attr of element.getAttributeNames()) {{
	            if (attr.startsWith("data-") && attr !== "data-testid") {{
	              const value = element.getAttribute(attr);
	              if (value && value.length <= 80) candidates.push(`[${{attr}}="${{cssEscape(value)}}"]`);
	            }}
	          }}
	          const id = element.getAttribute("id");
	          if (id) candidates.push(`#${{cssEscape(id)}}`);
	          const href = element.getAttribute("href");
	          if (href) candidates.push(`${{element.tagName.toLowerCase()}}[href="${{cssEscape(href)}}"]`);
	          const name = element.getAttribute("name");
	          if (name) candidates.push(`${{element.tagName.toLowerCase()}}[name="${{cssEscape(name)}}"]`);
	          return [...new Set(candidates)].slice(0, 6);
	        }};
	        const selector = [
	          "a",
	          "button",
	          "input",
	          "textarea",
	          "select",
	          "summary",
	          "[role]",
	          "[data-testid]",
	          "[onclick]",
	          "[contenteditable='true']",
	        ].join(",");
	        const targets = [];
	        for (const element of Array.from(document.querySelectorAll(selector))) {{
	          if (!visible(element)) continue;
	          const box = element.getBoundingClientRect();
	          const candidates = candidatesFor(element);
	          targets.push({{
	            id: `target-${{targets.length + 1}}`,
	            tag: element.tagName.toLowerCase(),
	            role: element.getAttribute("role") || null,
	            name: element.getAttribute("aria-label") || textOf(element) || null,
	            text: textOf(element) || null,
	            testId: element.getAttribute("data-testid") || null,
	            href: element.getAttribute("href") || null,
	            selector: candidates[0] || null,
	            candidates,
	            box: {{
	              x: Math.round(box.x),
	              y: Math.round(box.y),
	              width: Math.round(box.width),
	              height: Math.round(box.height),
	            }},
	          }});
	          if (targets.length >= limit) break;
	        }}
	        return {{
	          documentText: trim(document.body ? document.body.innerText : "", Math.max(textMax * 8, 800)),
	          targets,
	        }};
	      }},
	      {{ limit, textMax }}
	    );
	    return {{
	      url: page.url(),
	      title: await page.title(),
	      documentText: dom.documentText,
	      targets: dom.targets,
	    }};
	  }},
	  async goto(url, options = {{}}) {{
	    await page.goto(url, {{
	      waitUntil: options.waitUntil || "domcontentloaded",
	      timeout: options.timeoutMs || 30_000,
	    }});
	    historyPos += 1;
	    historyMax = historyPos;
	    return await this.observe(options);
	  }},
	  async click(target, options = {{}}) {{
	    const locator = await this.resolveTarget(target);
	    await locator.click(options);
	    return await this.observe(options);
	  }},
	  async fill(target, value, options = {{}}) {{
	    const locator = await this.resolveTarget(target);
	    await locator.fill(String(value), options);
	    return await this.observe(options);
	  }},
	  async press(target, key, options = {{}}) {{
	    const locator = await this.resolveTarget(target);
	    await locator.press(String(key), options);
	    return await this.observe(options);
	  }},
	  async screenshot(options = {{}}) {{
	    const buffer = await page.screenshot({{ fullPage: !!options.fullPage }});
	    return {{ mimeType: "image/png", base64: buffer.toString("base64") }};
	  }},
	  async logsFor(levels = ["error", "warning", "warn"]) {{
	    const wanted = new Set(levels);
	    return logs.filter((entry) => wanted.has(entry.level));
	  }},
	}};
	globalThis.ctoxBrowser = ctoxBrowserApi;

	const runAutomation = async (source, automationTimeoutMs) => {{
	  logs.length = 0;
	  let timeoutHandle = null;
	  try {{
	    const AsyncFunction = Object.getPrototypeOf(async function () {{}}).constructor;
	    const userFunction = new AsyncFunction(String(source || ""));
	    const result = await Promise.race([
	      userFunction(),
	      new Promise((_, reject) =>
	        timeoutHandle = setTimeout(
	          () => reject(new Error(`browser automation timed out after ${{automationTimeoutMs}}ms`)),
	          automationTimeoutMs
	        )
	      ),
	    ]);
	    if (timeoutHandle) {{
	      clearTimeout(timeoutHandle);
	      timeoutHandle = null;
	    }}
	    return {{
	      ok: true,
	      tool: "ctox_browser_automation",
	      session_mode: "business-os-persistent",
	      result: safeSerialize(result),
	      logs: safeSerialize(logs),
	      page: await pageMetadata(),
	      nav: await navState(),
	    }};
	  }} catch (error) {{
	    if (timeoutHandle) {{
	      clearTimeout(timeoutHandle);
	      timeoutHandle = null;
	    }}
	    return {{
	      ok: false,
	      tool: "ctox_browser_automation",
	      session_mode: "business-os-persistent",
	      error: (error && error.stack) || String(error),
	      logs: safeSerialize(logs),
	      page: await pageMetadata(),
	      nav: await navState(),
	    }};
	  }}
	}};

	respond({{ ready: true }});

const rl = readline.createInterface({{ input: process.stdin }});
for await (const line of rl) {{
  const text = line.trim();
  if (!text) continue;
  let message;
  try {{
    message = JSON.parse(text);
  }} catch {{
    continue;
  }}
  const id = message.id;
  const op = message.op;
  const timeoutMs = Number(message.timeoutMs || 30000);
  try {{
    if (op === "navigate") {{
        await page.goto(await assertAllowedUrl(message.url || "about:blank"), {{
        waitUntil: message.waitUntil || "domcontentloaded",
        timeout: timeoutMs,
      }});
      historyPos += 1;
      historyMax = historyPos;
      respond({{ id, ok: true, nav: await navState() }});
    }} else if (op === "reload") {{
      await page.reload({{ waitUntil: "domcontentloaded", timeout: timeoutMs }});
      respond({{ id, ok: true, nav: await navState() }});
    }} else if (op === "back") {{
      if (historyPos > 0) {{
        await page.goBack({{ waitUntil: "domcontentloaded", timeout: timeoutMs }}).catch(() => {{}});
        historyPos -= 1;
      }}
      respond({{ id, ok: true, nav: await navState() }});
    }} else if (op === "forward") {{
      if (historyPos < historyMax) {{
        await page.goForward({{ waitUntil: "domcontentloaded", timeout: timeoutMs }}).catch(() => {{}});
        historyPos += 1;
      }}
      respond({{ id, ok: true, nav: await navState() }});
    }} else if (op === "tab_open") {{
      if (context.pages().length >= 20) throw new Error("browser tab budget is exhausted");
      page = await context.newPage();
      ensureTabId(page, message.tabId || null);
      if (message.url) {{
        await page.goto(await assertAllowedUrl(message.url), {{ waitUntil: "domcontentloaded", timeout: timeoutMs }});
      }}
      respond({{ id, ok: true, nav: await navState() }});
    }} else if (op === "tab_activate") {{
      const selected = context.pages().find((candidate) => ensureTabId(candidate) === message.tabId);
      if (!selected) throw new Error("browser tab not found");
      page = selected;
      respond({{ id, ok: true, nav: await navState() }});
    }} else if (op === "tab_close") {{
      const selected = context.pages().find((candidate) => ensureTabId(candidate) === message.tabId);
      if (!selected) throw new Error("browser tab not found");
      await selected.close();
      page = context.pages()[0] || await context.newPage();
      respond({{ id, ok: true, nav: await navState() }});
    }} else if (op === "dialog_respond") {{
      if (!pendingDialog) throw new Error("no browser dialog is pending");
      const dialog = pendingDialog;
      pendingDialog = null;
      if (message.accept) await dialog.accept(message.value == null ? undefined : String(message.value));
      else await dialog.dismiss();
      respond({{ id, ok: true, nav: await navState() }});
    }} else if (op === "upload") {{
      if (!message.filePath) throw new Error("server-derived upload path is required");
      const locator = page.locator(String(message.selector || "input[type=file]"));
      if (await locator.count() !== 1) throw new Error("upload selector must resolve to exactly one file input");
      await locator.setInputFiles(String(message.filePath));
      respond({{ id, ok: true, nav: await navState() }});
    }} else if (op === "credential_fill") {{
      const selector = String(message.selector || "");
      if (!selector) throw new Error("credential selector is required");
      const locator = page.locator(selector);
      if (await locator.count() !== 1) throw new Error("credential selector must resolve to exactly one field");
      await locator.fill(String(message.value || ""));
      message.value = "[redacted]";
      respond({{ id, ok: true, nav: await navState() }});
    }} else if (op === "clipboard_copy") {{
      const clipboardText = await page.evaluate(() => String(globalThis.getSelection?.()?.toString?.() || ""));
      respond({{ id, ok: true, clipboardText, nav: await navState() }});
    }} else if (op === "clipboard_paste") {{
      await page.keyboard.insertText(String(message.value || ""));
      message.value = "[redacted]";
      respond({{ id, ok: true, nav: await navState() }});
    }} else if (op === "webauthn_respond") {{
      if (!pendingWebAuthn) throw new Error("no WebAuthn ceremony is pending");
      const state = await ensureWebAuthn(page);
      for (const credential of Array.isArray(message.credentials) ? message.credentials : []) {{
        await state.client.send("WebAuthn.addCredential", {{ authenticatorId: state.authenticatorId, credential }}).catch(() => {{}});
      }}
      if (!message.accept) {{
        await state.client.send("WebAuthn.removeVirtualAuthenticator", {{ authenticatorId: state.authenticatorId }}).catch(() => {{}});
        webAuthnSessions.delete(page);
        pendingWebAuthn = null;
        await ensureWebAuthn(page).catch(() => {{}});
        respond({{ id, ok: true, accepted: false, credentials: [], nav: await navState() }});
        continue;
      }}
      await state.client.send("WebAuthn.setAutomaticPresenceSimulation", {{ authenticatorId: state.authenticatorId, enabled: true }});
      await new Promise((resolve) => setTimeout(resolve, 750));
      const exported = await state.client.send("WebAuthn.getCredentials", {{ authenticatorId: state.authenticatorId }});
      await state.client.send("WebAuthn.setAutomaticPresenceSimulation", {{ authenticatorId: state.authenticatorId, enabled: false }});
      const rpId = pendingWebAuthn.rp_id;
      pendingWebAuthn = null;
      respond({{ id, ok: true, rpId, credentials: exported.credentials || [], nav: await navState() }});
    }} else if (op === "http_auth_respond") {{
      if (!pendingHttpAuth) throw new Error("no HTTP authentication challenge is pending");
      const challenge = pendingHttpAuth;
      pendingHttpAuth = null;
      await challenge.client.send("Fetch.continueWithAuth", {{
        requestId: challenge.request_id,
        authChallengeResponse: message.accept
          ? {{ response: "ProvideCredentials", username: String(message.username || ""), password: String(message.password || "") }}
          : {{ response: "CancelAuth" }},
      }});
      message.username = "[redacted]";
      message.password = "[redacted]";
      respond({{ id, ok: true, accepted: Boolean(message.accept), nav: await navState() }});
    }} else if (op === "permission_respond") {{
      if (!pendingPermission) throw new Error("no browser permission request is pending");
      const request = pendingPermission;
      pendingPermission = null;
      const allowed = ["camera", "microphone", "geolocation", "notifications"];
      if (!allowed.includes(request.kind)) throw new Error("unsupported browser permission kind");
      if (message.accept) {{
        await context.grantPermissions([request.kind], {{ origin: request.origin }});
        setTimeout(() => context.clearPermissions().catch(() => {{}}), 60_000).unref?.();
      }}
      await page.evaluate((input) => globalThis.__ctoxResolvePermission?.(input.kind, input.accept), {{ kind: request.kind, accept: Boolean(message.accept) }});
      respond({{ id, ok: true, accepted: Boolean(message.accept), permission: request.kind, nav: await navState() }});
    }} else if (op === "input") {{
      const applied = await applyInput(message.events);
      respond({{ id, ok: true, applied, nav: await navState() }});
    }} else if (op === "screenshot") {{
      respond({{ id, ok: true, screenshot: await screenshot(message), nav: await navState() }});
    }} else if (op === "nav_state") {{
      respond({{ id, ok: true, nav: await navState() }});
    }} else if (op === "viewport") {{
      await page.setViewportSize({{
        width: Number(message.w || VIEWPORT_W),
        height: Number(message.h || VIEWPORT_H),
      }});
      respond({{ id, ok: true }});
	    }} else if (op === "observe") {{
	      respond({{ id, ok: true, observed: await observe(message.limit, message.textMax) }});
	    }} else if (op === "automation") {{
	      const automationTimeoutMs = Math.max(1000, Math.min(300000, Number(message.timeoutMs || 30000)));
	      const result = await runAutomation(message.source, automationTimeoutMs);
	      respond({{ id, ...result }});
	    }} else if (op === "close") {{
	      respond({{ id, ok: true }});
	      try {{ await context.close(); }} catch {{}}
      process.exit(0);
    }} else {{
      respond({{ id, ok: false, error: `unknown op ${{op}}` }});
    }}
  }} catch (error) {{
    let nav = null;
    try {{ nav = await navState(); }} catch {{}}
    respond({{ id, ok: false, error: (error && error.message) || String(error), nav }});
  }}
}}
"#
    ))
}

fn build_browser_capture_runner_script(
    cdp_url: &str,
    target_url: &str,
    out_dir: &Path,
    timeout_ms: u64,
) -> Result<String> {
    let encoded_cdp_url =
        serde_json::to_string(cdp_url).context("failed to encode browser capture cdp url")?;
    let encoded_target_url =
        serde_json::to_string(target_url).context("failed to encode browser capture target url")?;
    let encoded_out_dir = serde_json::to_string(&out_dir.display().to_string())
        .context("failed to encode browser capture output dir")?;
    let target = Url::parse(target_url).context("failed to parse browser capture target URL")?;
    let host = target
        .host_str()
        .context("browser capture target URL requires a host")?;
    let homepage_url = match target.port() {
        Some(port) => format!("{}://{host}:{port}/", target.scheme()),
        None => format!("{}://{host}/", target.scheme()),
    };
    let encoded_homepage_url = serde_json::to_string(&homepage_url)
        .context("failed to encode browser capture homepage url")?;
    Ok(format!(
        r#"import fs from "node:fs/promises";
import process from "node:process";

const {{ chromium }} = await import("patchright");

const cdpUrl = {encoded_cdp_url};
const targetUrl = {encoded_target_url};
const homepageUrl = {encoded_homepage_url};
const outDir = {encoded_out_dir};
const timeoutMs = {timeout_ms};
const startTs = Date.now();
const cdpEvents = [];
const playwrightEvents = [];

const nowMs = () => Date.now() - startTs;

const pushEvent = (bucket, kind, payload) => {{
  bucket.push({{
    t_ms: nowMs(),
    kind,
    payload,
  }});
}};

const safe = (value, depth = 0, seen = new WeakSet()) => {{
  if (depth > 6) return "[depth limit]";
  if (value === null || value === undefined) return value ?? null;
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") return value;
  if (typeof value === "bigint") return value.toString();
  if (Array.isArray(value)) return value.slice(0, 256).map((item) => safe(item, depth + 1, seen));
  if (typeof value === "object") {{
    if (seen.has(value)) return "[circular]";
    seen.add(value);
    const out = {{}};
    for (const [key, item] of Object.entries(value).slice(0, 256)) {{
      out[key] = safe(item, depth + 1, seen);
    }}
    return out;
  }}
  return String(value);
}};

const lower = (text) => String(text || "").toLowerCase();
const responseMarkers = (html) => {{
  const lowered = lower(html);
  return {{
    dataVed: lowered.includes("data-ved"),
    sorry: lowered.includes("/sorry/") || lowered.includes("sorry.google.com") || lowered.includes("unusual traffic"),
    captcha: lowered.includes("captcha-form"),
    enablejs: lowered.includes("enablejs"),
  }};
}};

let browser = null;
let context = null;
let page = null;

try {{
  browser = await chromium.connectOverCDP(cdpUrl);
  context = browser.contexts()[0];
  if (!context) {{
    throw new Error("CDP capture did not expose a browser context");
  }}
  page = context.pages()[0] || await context.newPage();
  const client = await context.newCDPSession(page);
  await client.send("Network.enable");
  try {{
    await client.send("Security.enable");
  }} catch {{
    // ignore missing security domain support
  }}

  for (const name of [
    "Network.requestWillBeSent",
    "Network.requestWillBeSentExtraInfo",
    "Network.responseReceived",
    "Network.responseReceivedExtraInfo",
    "Network.loadingFinished",
    "Network.loadingFailed",
    "Security.securityStateChanged"
  ]) {{
    client.on(name, (payload) => pushEvent(cdpEvents, name, safe(payload)));
  }}

  page.on("request", async (request) => {{
    let headers = null;
    try {{
      headers = typeof request.allHeaders === "function" ? await request.allHeaders() : request.headers();
    }} catch {{}}
    pushEvent(playwrightEvents, "request", safe({{
      url: request.url(),
      method: request.method(),
      resourceType: request.resourceType(),
      headers,
    }}));
  }});

  page.on("response", async (response) => {{
    let headers = null;
    try {{
      headers = typeof response.allHeaders === "function" ? await response.allHeaders() : response.headers();
    }} catch {{}}
    pushEvent(playwrightEvents, "response", safe({{
      url: response.url(),
      status: response.status(),
      requestResourceType: response.request().resourceType(),
      headers,
    }}));
  }});

  page.on("requestfailed", (request) => {{
    pushEvent(playwrightEvents, "requestfailed", safe({{
      url: request.url(),
      method: request.method(),
      resourceType: request.resourceType(),
      failure: request.failure(),
    }}));
  }});

  await page.goto(homepageUrl, {{ waitUntil: "domcontentloaded", timeout: timeoutMs }});
  await page.waitForTimeout(1200);
  const cookiesAfterHomepage = await context.cookies();

  await page.goto(targetUrl, {{ waitUntil: "domcontentloaded", timeout: timeoutMs }});
  await page.waitForTimeout(1800);

  const finalUrl = page.url();
  const title = await page.title();
  const html = await page.content();
  const cookiesAfterTarget = await context.cookies();
  const summary = {{
    ok: true,
    tool: "ctox_browser_capture",
    targetUrl,
    homepageUrl,
    finalUrl,
    title,
    markers: responseMarkers(html),
    cookiesAfterHomepage: safe(cookiesAfterHomepage),
    cookiesAfterTarget: safe(cookiesAfterTarget),
    counts: {{
      cdpEvents: cdpEvents.length,
      playwrightEvents: playwrightEvents.length,
    }},
    artifacts: {{
      outDir,
      summaryPath: `${{outDir}}/capture-summary.json`,
      cdpEventsPath: `${{outDir}}/cdp-events.json`,
      playwrightEventsPath: `${{outDir}}/playwright-events.json`,
      htmlPath: `${{outDir}}/page.html`,
      netlogPath: `${{outDir}}/chrome-netlog.json`,
    }},
  }};

  await fs.writeFile(`${{outDir}}/page.html`, html, "utf8");
  await fs.writeFile(`${{outDir}}/cdp-events.json`, JSON.stringify(cdpEvents, null, 2), "utf8");
  await fs.writeFile(`${{outDir}}/playwright-events.json`, JSON.stringify(playwrightEvents, null, 2), "utf8");
  await fs.writeFile(`${{outDir}}/capture-summary.json`, JSON.stringify(summary, null, 2), "utf8");
  process.stdout.write(JSON.stringify(summary));
}} catch (error) {{
  const failure = {{
    ok: false,
    tool: "ctox_browser_capture",
    targetUrl,
    homepageUrl,
    error: (error && error.stack) || String(error),
    counts: {{
      cdpEvents: cdpEvents.length,
      playwrightEvents: playwrightEvents.length,
    }},
    artifacts: {{
      outDir,
      summaryPath: `${{outDir}}/capture-summary.json`,
      cdpEventsPath: `${{outDir}}/cdp-events.json`,
      playwrightEventsPath: `${{outDir}}/playwright-events.json`,
      netlogPath: `${{outDir}}/chrome-netlog.json`,
    }},
  }};
  await fs.writeFile(`${{outDir}}/cdp-events.json`, JSON.stringify(cdpEvents, null, 2), "utf8");
  await fs.writeFile(`${{outDir}}/playwright-events.json`, JSON.stringify(playwrightEvents, null, 2), "utf8");
  await fs.writeFile(`${{outDir}}/capture-summary.json`, JSON.stringify(failure, null, 2), "utf8");
  process.stdout.write(JSON.stringify(failure));
}} finally {{
  try {{
    if (browser) {{
      await browser.close();
    }}
  }} catch {{
    // ignore close errors; Rust will still reap the chrome process
  }}
}}"#
    ))
}

fn trim_text(raw: &str, max_chars: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        trimmed.chars().take(max_chars).collect::<String>() + "..."
    }
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::browser_doctor_report;
    use super::build_browser_capture_runner_script;
    use super::build_browser_runner_script;
    use super::build_persistent_browser_runner_script;
    use super::capture_chrome_extra_args;
    #[cfg(unix)]
    use super::cleanup_stale_chromium_profile_locks;
    use super::ensure_reference_package_json;
    use super::find_playwright_chromium_executable_in;
    use super::parse_browser_automation_source;
    use super::parse_node_major_version;
    use super::resolve_root_relative_path;
    use super::select_capture_browser_executable;
    #[cfg(unix)]
    use super::PersistentBrowserHandle;
    use std::fs;
    #[cfg(unix)]
    use std::io::BufReader;
    use std::path::PathBuf;
    #[cfg(unix)]
    use std::process::Command;
    #[cfg(unix)]
    use std::process::Stdio;
    #[cfg(unix)]
    use std::time::Duration;
    #[cfg(unix)]
    use std::time::Instant;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    fn temp_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ctox-browser-{label}-{unique}"))
    }

    #[test]
    fn finds_playwright_chromium_linux64_executable() {
        let dir = temp_path("linux64-cache");
        let executable = dir.join("chromium-1217/chrome-linux64/chrome");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"").unwrap();
        assert_eq!(
            find_playwright_chromium_executable_in(&dir),
            Some(executable)
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_playwright_chromium_win64_executable() {
        let dir = temp_path("win64-cache");
        let executable = dir.join("chromium-1217/chrome-win64/chrome.exe");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"").unwrap();
        assert_eq!(
            find_playwright_chromium_executable_in(&dir),
            Some(executable)
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ensure_reference_package_json_writes_patchright_dependency() {
        let dir = temp_path("package-json");
        fs::create_dir_all(&dir).unwrap();
        let created = ensure_reference_package_json(&dir).unwrap();
        let raw = fs::read(dir.join("package.json")).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert!(created);
        assert_eq!(
            value["dependencies"]["patchright"].as_str(),
            Some("^1.55.0")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn browser_doctor_report_uses_explicit_reference_dir() {
        let root = temp_path("doctor-root");
        let reference_dir = root.join("custom-reference");
        fs::create_dir_all(&reference_dir).unwrap();
        let report = browser_doctor_report(&root, Some(reference_dir.clone())).unwrap();
        assert_eq!(
            report
                .get("reference_dir")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            Some(reference_dir.display().to_string())
        );
        assert_eq!(
            report
                .get("automation_ready")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn browser_automation_pragma_overrides_timeout() {
        let (directive, body) = parse_browser_automation_source(
            "// ctox-browser: timeout_ms=15000\nawait page.goto('https://example.com');",
        )
        .unwrap();
        assert_eq!(directive.timeout_ms, Some(15_000));
        assert_eq!(body, "await page.goto('https://example.com');");
    }

    #[test]
    fn browser_runner_script_embeds_timeout_and_source() {
        let script =
            build_browser_runner_script("return await page.title();", 12_345, None).unwrap();
        assert!(script.contains("const timeoutMs = 12345;"));
        assert!(script.contains("const userSource = \"return await page.title();\";"));
    }

    #[test]
    fn browser_runner_script_exposes_agent_friendly_api() {
        let script = build_browser_runner_script(
            "return await ctoxBrowser.observe({ limit: 10 });",
            12_345,
            None,
        )
        .unwrap();
        assert!(script.contains("globalThis.ctoxBrowser = ctoxBrowserApi;"));
        assert!(script.contains("async observe(options = {})"));
        assert!(script.contains("async resolveTarget(target)"));
        assert!(script.contains("async click(target, options = {})"));
        assert!(script.contains("async screenshot(options = {})"));
        assert!(script.contains("const detectionSnapshot = async (page)"));
        assert!(script.contains("cloudflare_challenge"));
        let path = std::env::temp_dir().join(format!(
            "ctox-browser-runner-detection-{}.mjs",
            std::process::id()
        ));
        std::fs::write(&path, script).unwrap();
        let status = std::process::Command::new("node")
            .arg("--check")
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success(), "generated browser runner must parse");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persistent_browser_runner_exposes_session_automation_op() {
        let script = build_persistent_browser_runner_script(
            1280,
            720,
            None,
            std::path::Path::new("/tmp/ctox-browser-test-profile"),
            std::path::Path::new("/tmp/ctox-browser-test-downloads"),
            &[],
        )
        .unwrap();
        assert!(script.contains("op === \"automation\""));
        assert!(script.contains("session_mode: \"business-os-persistent\""));
        assert!(script.contains("globalThis.ctoxBrowser = ctoxBrowserApi;"));
        assert!(script.contains("async observe(options = {})"));
        assert!(script.contains("launchPersistentContext"));
        assert!(script.contains("blocked browser egress host"));
        assert!(script.contains("op === \"webauthn_respond\""));
        assert!(script.contains("op === \"credential_fill\""));
        assert!(script.contains("op === \"clipboard_copy\""));
        let path = std::env::temp_dir().join(format!(
            "ctox-persistent-browser-runner-{}.mjs",
            std::process::id()
        ));
        std::fs::write(&path, script).unwrap();
        let status = std::process::Command::new("node")
            .arg("--check")
            .arg(&path)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "generated persistent browser runner must parse"
        );
        let _ = std::fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn persistent_browser_request_timeout_terminates_unresponsive_process() {
        use std::os::unix::process::CommandExt;

        let runner_path = temp_path("unresponsive-runner");
        fs::write(&runner_path, b"").unwrap();
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("while read line; do sleep 300; done")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut handle = PersistentBrowserHandle {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 0,
            runner_path,
            profile_dir: None,
            downloads_dir: None,
            remove_profile_on_close: false,
        };

        let started = Instant::now();
        let error = handle
            .request_with_timeout(
                "automation",
                serde_json::json!({}),
                Duration::from_millis(100),
            )
            .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn persistent_browser_startup_timeout_terminates_unresponsive_process() {
        use std::os::unix::process::CommandExt;

        let runner_path = temp_path("unresponsive-startup-runner");
        fs::write(&runner_path, b"").unwrap();
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("sleep 300")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut handle = PersistentBrowserHandle {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 0,
            runner_path,
            profile_dir: None,
            downloads_dir: None,
            remove_profile_on_close: false,
        };

        let started = Instant::now();
        let error = handle
            .wait_until_ready(Duration::from_millis(100))
            .unwrap_err();
        assert!(error.to_string().contains("did not become ready"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn stale_chromium_profile_lock_is_removed_but_live_lock_is_preserved() {
        use std::os::unix::fs::symlink;

        let profile = temp_path("profile-lock-cleanup");
        fs::create_dir_all(&profile).unwrap();
        let lock = profile.join("SingletonLock");
        symlink("host-2147483647", &lock).unwrap();
        fs::write(profile.join("SingletonCookie"), b"stale").unwrap();
        cleanup_stale_chromium_profile_locks(&profile).unwrap();
        assert!(fs::symlink_metadata(&lock).is_err());
        assert!(!profile.join("SingletonCookie").exists());

        symlink(format!("host-{}", std::process::id()), &lock).unwrap();
        cleanup_stale_chromium_profile_locks(&profile).unwrap();
        assert!(fs::symlink_metadata(&lock).is_ok());
        let _ = fs::remove_dir_all(profile);
    }

    #[test]
    fn parses_node_major_version_from_semver_output() {
        assert_eq!(parse_node_major_version("v12.22.9"), Some(12));
        assert_eq!(parse_node_major_version("18.19.1"), Some(18));
        assert_eq!(parse_node_major_version("not-a-version"), None);
    }

    #[test]
    fn capture_chrome_uses_headless_args_without_gui() {
        let args = capture_chrome_extra_args(true, true);
        assert!(args.contains(&"--headless=new"));
        assert!(args.contains(&"--disable-gpu"));
        assert!(args.contains(&"--disable-dev-shm-usage"));
        assert!(args.contains(&"--no-sandbox"));
    }

    #[test]
    fn capture_chrome_keeps_headed_mode_when_gui_exists() {
        let args = capture_chrome_extra_args(false, false);
        assert!(!args.contains(&"--headless=new"));
        assert!(!args.contains(&"--disable-gpu"));
    }

    #[test]
    fn browser_capture_resolves_relative_output_under_root() {
        let root = PathBuf::from("/tmp/ctox-root");
        assert_eq!(
            resolve_root_relative_path(&root, PathBuf::from("runtime/capture")),
            PathBuf::from("/tmp/ctox-root/runtime/capture")
        );
        assert_eq!(
            resolve_root_relative_path(&root, PathBuf::from("/tmp/absolute-capture")),
            PathBuf::from("/tmp/absolute-capture")
        );
    }

    #[test]
    fn browser_capture_uses_playwright_chrome_for_testing() {
        let selected =
            select_capture_browser_executable(Some(PathBuf::from("/tmp/chrome-for-testing")))
                .expect("browser executable");
        assert_eq!(selected.0, "playwright-cache");
        assert_eq!(selected.1, PathBuf::from("/tmp/chrome-for-testing"));
    }

    #[test]
    fn browser_capture_rejects_missing_playwright_chrome_for_testing() {
        assert!(select_capture_browser_executable(None).is_none());
    }

    #[test]
    fn browser_capture_homepage_preserves_explicit_port() {
        let script = build_browser_capture_runner_script(
            "http://127.0.0.1:12345",
            "http://127.0.0.1:8765/browser.html",
            PathBuf::from("/tmp/capture").as_path(),
            5000,
        )
        .unwrap();
        assert!(script.contains("const homepageUrl = \"http://127.0.0.1:8765/\";"));
    }
}
