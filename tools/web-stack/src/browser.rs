use anyhow::Context;
use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use std::fs;
use std::io;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use url::Url;

const DEFAULT_REFERENCE_RELATIVE_DIR: &str = "runtime/browser/interactive-reference";
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
    playwright_dependency_declared: bool,
    playwright_dependency_installed: bool,
    playwright_browser_cache_dir: PathBuf,
    playwright_browser_installed: bool,
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
            let report =
                install_reference(&reference_dir, !skip_npm_install, install_browser)?;
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

pub fn prepare_browser_environment(
    root: &Path,
    options: &BrowserPrepareOptions,
) -> Result<serde_json::Value> {
    let reference_dir = options
        .dir
        .clone()
        .unwrap_or_else(|| root.join(DEFAULT_REFERENCE_RELATIVE_DIR));
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
    let reference_dir = request
        .dir
        .clone()
        .unwrap_or_else(|| root.join(DEFAULT_REFERENCE_RELATIVE_DIR));
    fs::create_dir_all(&reference_dir).with_context(|| {
        format!(
            "failed to create browser automation reference dir {}",
            reference_dir.display()
        )
    })?;
    let _ = ensure_reference_package_json(&reference_dir)?;
    let doctor = build_doctor_report(&reference_dir)?;
    if !doctor.automation_ready {
        anyhow::bail!(
            "browser automation runtime is not ready for {}. Run `ctox web browser-prepare --dir {} --install-reference [--install-browser]` first.",
            reference_dir.display(),
            reference_dir.display()
        );
    }
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

    serde_json::from_str(&stdout).with_context(|| {
        format!(
            "browser automation runtime produced invalid json: {}",
            trim_text(&stdout, 400)
        )
    })
}

pub fn capture_browser_transport(root: &Path, request: &BrowserCaptureRequest) -> Result<Value> {
    let reference_dir = request
        .dir
        .clone()
        .unwrap_or_else(|| root.join(DEFAULT_REFERENCE_RELATIVE_DIR));
    fs::create_dir_all(&reference_dir).with_context(|| {
        format!(
            "failed to create browser automation reference dir {}",
            reference_dir.display()
        )
    })?;
    let _ = ensure_reference_package_json(&reference_dir)?;
    let doctor = build_doctor_report(&reference_dir)?;
    if !doctor.automation_ready {
        anyhow::bail!(
            "browser capture runtime is not ready for {}. Run `ctox web browser-prepare --dir {} --install-reference [--install-browser]` first.",
            reference_dir.display(),
            reference_dir.display()
        );
    }
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
    let playwright_dependency_declared = read_playwright_dependency_declared(reference_dir)?;
    let playwright_dependency_installed = reference_dir
        .join("node_modules")
        .join("playwright")
        .join("package.json")
        .is_file();
    let playwright_browser_cache_dir = playwright_browser_cache_dir(reference_dir);
    let chromium_fallback_executable =
        find_browser_executable(reference_dir).map(|value| value.display().to_string());
    let playwright_browser_installed = chromium_fallback_executable.is_some();
    let node = detect_tool("node", &["--version"]);
    let npm = detect_tool("npm", &["--version"]);
    let npx = detect_tool("npx", &["--version"]);
    let node_major = node.version.as_deref().and_then(parse_node_major_version);
    let node_version_compatible = node_major
        .map(|major| major >= MINIMUM_NODE_MAJOR)
        .unwrap_or(false);
    let ok = node.available && npm.available && npx.available;
    let smoke = if ok
        && node_version_compatible
        && playwright_dependency_installed
        && playwright_browser_installed
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
    let automation_ready = ok
        && node_version_compatible
        && playwright_dependency_installed
        && playwright_browser_installed
        && smoke.ok;
    Ok(BrowserDoctorReport {
        ok,
        reference_dir: reference_dir.to_path_buf(),
        package_json_exists,
        node_modules_exists,
        minimum_node_major: MINIMUM_NODE_MAJOR,
        node_major,
        node_version_compatible,
        playwright_dependency_declared,
        playwright_dependency_installed,
        playwright_browser_cache_dir,
        playwright_browser_installed,
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

fn run_browser_smoke(
    reference_dir: &Path,
    chromium_fallback_executable: Option<&str>,
) -> BrowserSmokeReport {
    const TIMEOUT_MS: u64 = 8_000;
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
const {{ chromium }} = await import("playwright");
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
    if run_npm_install {
        run_command(
            reference_dir,
            "npm",
            &["install", "playwright"],
            "failed to install playwright reference",
        )?;
    }
    if install_browser {
        let browser_cache_dir = playwright_browser_cache_dir(reference_dir);
        run_command_with_env(
            reference_dir,
            "npx",
            &["playwright", "install", "chromium"],
            &[("PLAYWRIGHT_BROWSERS_PATH", browser_cache_dir.as_path())],
            "failed to install Playwright chromium browser",
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

fn ensure_reference_package_json(reference_dir: &Path) -> Result<bool> {
    let package_json_path = reference_dir.join("package.json");
    if package_json_path.exists() {
        return Ok(false);
    }
    let package_json = json!({
        "name": "ctox-interactive-browser-reference",
        "private": true,
        "type": "module",
        "description": "CTOX-owned Playwright runtime workspace for browser automation.",
        "scripts": {
            "doctor": "node -e \"import('playwright').then(() => console.log('playwright import ok')).catch((error) => { console.error(error); process.exit(1); })\"",
            "install:chromium": "playwright install chromium"
        },
        "dependencies": {
            "playwright": "^1.53.0"
        }
    });
    fs::write(
        &package_json_path,
        serde_json::to_vec_pretty(&package_json)?,
    )
    .with_context(|| format!("failed to write {}", package_json_path.display()))?;
    Ok(true)
}

fn read_playwright_dependency_declared(reference_dir: &Path) -> Result<bool> {
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
        .and_then(|value| value.get("playwright"))
        .and_then(serde_json::Value::as_str)
        .is_some())
}

fn run_command(cwd: &Path, program: &str, args: &[&str], error_message: &str) -> Result<()> {
    run_command_with_env(cwd, program, args, &[], error_message)
}

fn command_output_with_timeout(mut command: Command, timeout: Duration) -> Result<Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to launch command")?;
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return child
                .wait_with_output()
                .context("failed to collect command output");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("timed out after {}ms", timeout.as_millis());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn run_command_with_env(
    cwd: &Path,
    program: &str,
    args: &[&str],
    envs: &[(&str, &Path)],
    error_message: &str,
) -> Result<()> {
    let mut command = Command::new(program);
    command.current_dir(cwd).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command
        .output()
        .with_context(|| format!("{error_message}: failed to launch `{program}`"))?;
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

fn find_command_on_path(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let path = PathBuf::from(program);
        return path.is_file().then_some(path);
    }
    let path_env = std::env::var_os("PATH")?;
    std::env::split_paths(&path_env)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

fn resolve_reference_dir(root: &Path, args: &[String]) -> PathBuf {
    find_flag_value(args, "--dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(DEFAULT_REFERENCE_RELATIVE_DIR))
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
        "const {{ chromium }} = await import(\"playwright\");\nconst browser = await chromium.launch({launch_options});\nconst context = await browser.newContext();\nconst page = await context.newPage();\nawait page.goto(\"http://127.0.0.1:3000\", {{ waitUntil: \"domcontentloaded\" }});\nconsole.log(await page.title());\nawait browser.close();"
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

fn playwright_browser_cache_dir(reference_dir: &Path) -> PathBuf {
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

const emit = async (payload) => {{
  process.stdout.write(JSON.stringify(payload));
}};

const {{ chromium, firefox, webkit }} = await import("playwright");
const launchOptions = {{
  headless: true,
}};
if (fallbackExecutable) {{
  launchOptions.executablePath = fallbackExecutable;
}}
const contextOptions = {{
  viewport: {{ width: 1600, height: 900 }},
}};

const profileDir = path.join(process.cwd(), ".ctox-browser-profile");
const browser = await chromium.launch(launchOptions);
const context = await browser.newContext(contextOptions);
const page = await context.newPage();
globalThis.chromium = chromium;
globalThis.firefox = firefox;
globalThis.webkit = webkit;
globalThis.context = context;
globalThis.page = page;
globalThis.browser = browser;
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

const {{ chromium }} = await import("playwright");

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
    use super::build_browser_capture_runner_script;
    use super::build_browser_runner_script;
    use super::capture_chrome_extra_args;
    use super::ensure_reference_package_json;
    use super::find_playwright_chromium_executable_in;
    use super::parse_browser_automation_source;
    use super::parse_node_major_version;
    use super::resolve_root_relative_path;
    use super::select_capture_browser_executable;
    use std::fs;
    use std::path::PathBuf;
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
    fn ensure_reference_package_json_writes_playwright_dependency() {
        let dir = temp_path("package-json");
        fs::create_dir_all(&dir).unwrap();
        let created = ensure_reference_package_json(&dir).unwrap();
        let raw = fs::read(dir.join("package.json")).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert!(created);
        assert_eq!(
            value["dependencies"]["playwright"].as_str(),
            Some("^1.53.0")
        );
        let _ = fs::remove_dir_all(&dir);
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
