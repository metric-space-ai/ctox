// Origin: CTOX
// License: Apache-2.0

use super::*;

pub(super) fn assess_current_context_health(
    root: &Path,
    db_path: &Path,
    conversation_id: i64,
    latest_prompt: Option<&str>,
) -> Option<context_health::ContextHealthSnapshot> {
    let max_context = runtime_kernel::InferenceRuntimeKernel::resolve(root)
        .ok()
        .map(|runtime| runtime.turn_context_tokens())
        .unwrap_or(131_072);
    context_health::assess_for_conversation(db_path, conversation_id, max_context, latest_prompt)
        .ok()
}

pub(super) fn clip_text(value: &str, max_chars: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut clipped = collapsed
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    clipped.push('…');
    clipped
}

pub(super) fn default_follow_up_thread_key(goal: &str) -> String {
    let digest = {
        use sha2::Digest;
        let bytes = sha2::Sha256::digest(goal.as_bytes());
        let hex = format!("{bytes:x}");
        hex[..12].to_string()
    };
    format!("queue/follow-up-{digest}")
}

pub(super) fn build_queue_guard_prompt(root: &Path, pending: usize) -> String {
    let ctox_bin = preferred_ctox_executable(root)
        .unwrap_or_else(|_| std::env::current_exe().unwrap_or_else(|_| root.join("ctox")));
    format!(
        "Use the queue-cleanup skill first. The CTOX service queue is under pressure with {pending} queued prompt(s). Before doing any normal work, inspect the service state for this root: {}. Prefer the local CLI binary `{}` with `status`, `schedule list`, and `queue list`. If that binary is unavailable, inspect `runtime/ctox_service.log` plus the runtime databases directly instead of assuming `ctox` is on PATH. Find the source of repeated or flooding work, pause or contain any schedule that is filling the queue, avoid duplicate follow-up tasks, and keep only the minimum safe next work moving. Use `ctox queue spill-candidates` to identify explicit spillover candidates, `ctox queue spill --message-key <key>` to park valid work in the internal ticket system, `ctox queue spills` to review parked work, and `ctox queue restore --message-key <key>` to rehydrate it later. Treat queue recovery as top priority and report what was paused, deduplicated, blocked, spilled, restored, or left active.",
        root.display(),
        ctox_bin.display()
    )
}

#[derive(Debug, Clone)]
pub(super) struct SystemdUnitStatus {
    pub(super) active: bool,
    pub(super) enabled: bool,
    pub(super) pid: Option<u32>,
}

#[derive(Debug, Clone)]
pub(super) struct LaunchdUnitStatus {
    pub(super) active: bool,
    pub(super) enabled: bool,
    pub(super) pid: Option<u32>,
}

/// TTL-cached variant of `systemd_unit_status` for UI-cadence polling. The
/// unit's enabled/active state changes on operator action, not between
/// sub-second refresh ticks; a fresh probe costs three systemctl spawns.
/// `start_background`/`stop_background` drop the cache on every exit path
/// so an in-process toggle is visible on the next poll.
fn systemd_unit_status_cache(
) -> &'static Mutex<Option<(Instant, PathBuf, Option<SystemdUnitStatus>)>> {
    static CACHE: OnceLock<Mutex<Option<(Instant, PathBuf, Option<SystemdUnitStatus>)>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn invalidate_systemd_unit_status_cache() {
    *systemd_unit_status_cache()
        .lock()
        .unwrap_or_else(|err| err.into_inner()) = None;
}

/// Drops the systemd probe cache when it goes out of scope. Held across the
/// service start/stop mutators so every exit path (including `?` errors
/// after a partial enable/stop) invalidates the cached unit state.
pub(super) struct SystemdCacheInvalidator;

impl Drop for SystemdCacheInvalidator {
    fn drop(&mut self) {
        invalidate_systemd_unit_status_cache();
    }
}

pub(super) fn systemd_unit_status_cached(
    root: &Path,
    ttl: Duration,
) -> Result<Option<SystemdUnitStatus>> {
    let cache = systemd_unit_status_cache();
    if let Some((probed_at, cached_root, cached)) =
        cache.lock().unwrap_or_else(|err| err.into_inner()).as_ref()
    {
        if cached_root.as_path() == root && probed_at.elapsed() < ttl {
            return Ok(cached.clone());
        }
    }
    let fresh = systemd_unit_status(root)?;
    *cache.lock().unwrap_or_else(|err| err.into_inner()) =
        Some((Instant::now(), root.to_path_buf(), fresh.clone()));
    Ok(fresh)
}

pub(super) fn systemd_unit_status(root: &Path) -> Result<Option<SystemdUnitStatus>> {
    if !systemd_user_available() || !systemd_user_unit_installed(root) {
        return Ok(None);
    }
    let active = match systemctl_user(["is-active", "--quiet", SYSTEMD_USER_UNIT_NAME]) {
        Ok(()) => true,
        Err(_) => false,
    };
    let enabled_output = systemctl_user_capture(["is-enabled", SYSTEMD_USER_UNIT_NAME])?;
    let enabled_stdout = String::from_utf8_lossy(&enabled_output.stdout)
        .trim()
        .to_string();
    let enabled = enabled_output.status.success()
        && matches!(
            enabled_stdout.as_str(),
            "enabled" | "enabled-runtime" | "static"
        );
    let pid_output = systemctl_user_capture([
        "show",
        SYSTEMD_USER_UNIT_NAME,
        "--property",
        "MainPID",
        "--value",
    ])?;
    let pid = if pid_output.status.success() {
        String::from_utf8_lossy(&pid_output.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
    } else {
        None
    };
    Ok(Some(SystemdUnitStatus {
        active,
        enabled,
        pid,
    }))
}

fn systemd_user_available() -> bool {
    cfg!(target_os = "linux")
        && Command::new("systemctl")
            .arg("--user")
            .arg("--version")
            .output()
            .is_ok()
}

pub(super) fn systemd_user_unit_installed(root: &Path) -> bool {
    if root.join("runtime/ctox_systemd_user.installed").exists() {
        return true;
    }
    let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        });
    let Some(config_home) = xdg_config_home else {
        return false;
    };
    let unit_path = config_home
        .join("systemd/user")
        .join(SYSTEMD_USER_UNIT_NAME);
    if !unit_path.exists() {
        return false;
    }
    let Ok(unit_text) = std::fs::read_to_string(&unit_path) else {
        return false;
    };
    let normalized_root = root.display().to_string();
    let working_directory = format!("WorkingDirectory={normalized_root}");
    let ctox_root_env = format!("Environment=CTOX_ROOT={normalized_root}");
    unit_text
        .lines()
        .map(str::trim)
        .any(|line| line == working_directory || line == ctox_root_env)
}

pub(super) fn launchd_unit_status(root: &Path) -> Result<Option<LaunchdUnitStatus>> {
    if !launchd_user_available() || !launchd_user_unit_installed(root) {
        return Ok(None);
    }
    let enabled = !launchd_unit_disabled().unwrap_or(false);
    let output = match launchctl_user_capture(vec!["print".to_string(), launchd_target_label()]) {
        Ok(output) => output,
        Err(_) => {
            return Ok(Some(LaunchdUnitStatus {
                active: false,
                enabled,
                pid: None,
            }));
        }
    };
    if !output.status.success() {
        return Ok(Some(LaunchdUnitStatus {
            active: false,
            enabled,
            pid: None,
        }));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let pid = parse_launchd_pid(&stdout);
    let active = pid.is_some_and(process_is_running)
        || stdout
            .lines()
            .map(str::trim)
            .any(|line| line == "state = running" || line == "state = spawn scheduled");
    Ok(Some(LaunchdUnitStatus {
        active,
        enabled,
        pid,
    }))
}

fn launchd_user_available() -> bool {
    cfg!(target_os = "macos")
        && Command::new("launchctl")
            .arg("help")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
}

pub(super) fn launchd_user_unit_installed(root: &Path) -> bool {
    if root.join(LAUNCHD_USER_MARKER_RELATIVE_PATH).exists() {
        return true;
    }
    let Some(plist_path) = launchd_plist_path() else {
        return false;
    };
    if !plist_path.exists() {
        return false;
    }
    let Ok(plist_text) = std::fs::read_to_string(&plist_path) else {
        return false;
    };
    let normalized_root = root.display().to_string();
    plist_text.contains(&normalized_root) || plist_text.contains(&xml_escape_text(&normalized_root))
}

fn launchd_unit_disabled() -> Result<bool> {
    let output = launchctl_user_capture(vec!["print-disabled".to_string(), launchd_user_domain()])?;
    if !output.status.success() {
        return Ok(false);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let label = format!("\"{LAUNCHD_USER_LABEL}\"");
    Ok(stdout.lines().map(str::trim).any(|line| {
        line.contains(&label)
            && line
                .rsplit_once("=>")
                .is_some_and(|(_, value)| value.trim().eq_ignore_ascii_case("true"))
    }))
}

pub(super) fn parse_launchd_pid(output: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        let (key, value) = line.trim().split_once('=')?;
        if key.trim() != "pid" {
            return None;
        }
        value.trim().parse::<u32>().ok().filter(|pid| *pid > 0)
    })
}

pub(super) fn launchd_bootstrap_and_start(root: &Path) -> Result<()> {
    let plist_path = launchd_plist_path().context("failed to resolve launchd plist path")?;
    if !plist_path.exists() {
        anyhow::bail!(
            "CTOX launchd plist is missing: {}. Run `ctox upgrade --dev` or reinstall CTOX to refresh the user service.",
            plist_path.display()
        );
    }
    let plist_display = plist_path.display().to_string();
    let domain = launchd_user_domain();
    let target = launchd_target_label();
    let _ = launchd_bootout();
    launchd_enable().context("failed to enable CTOX launchd service before bootstrap")?;
    launchctl_user(vec!["bootstrap".to_string(), domain.clone(), plist_display])?;
    launchd_enable().context("failed to enable CTOX launchd service after bootstrap")?;
    if launchd_unit_disabled().unwrap_or(false) {
        anyhow::bail!(
            "CTOX launchd service stayed disabled after bootstrap; run `launchctl enable {target}` or reinstall CTOX"
        );
    }
    if let Err(err) = launchctl_user(vec!["kickstart".to_string(), "-k".to_string(), target]) {
        eprintln!("ctox service: launchctl kickstart warning: {err:#}");
    }
    let marker = root.join(LAUNCHD_USER_MARKER_RELATIVE_PATH);
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(&marker, "installed\n")
        .with_context(|| format!("failed to update {}", marker.display()))?;
    Ok(())
}

fn launchd_enable() -> Result<()> {
    launchctl_user(vec!["enable".to_string(), launchd_target_label()])
}

pub(super) fn launchd_bootout() -> Result<()> {
    let output = launchctl_user_capture(vec!["bootout".to_string(), launchd_target_label()])?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = format!("{stderr}\n{stdout}");
    if message.contains("No such process")
        || message.contains("service is not loaded")
        || message.contains("Could not find service")
    {
        return Ok(());
    }
    anyhow::bail!("launchctl bootout failed: {}", message.trim())
}

pub(super) fn launchd_disable() -> Result<()> {
    let output = launchctl_user_capture(vec!["disable".to_string(), launchd_target_label()])?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };
    anyhow::bail!("launchctl disable failed: {message}")
}

fn launchd_plist_path() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    std::env::var_os("HOME").map(PathBuf::from).map(|home| {
        home.join("Library/LaunchAgents")
            .join(format!("{LAUNCHD_USER_LABEL}.plist"))
    })
}

pub(super) fn launchd_target_label() -> String {
    format!("{}/{}", launchd_user_domain(), LAUNCHD_USER_LABEL)
}

pub(super) fn launchd_user_domain() -> String {
    #[cfg(target_os = "macos")]
    {
        return format!("gui/{}", unsafe { geteuid() });
    }
    #[cfg(not(target_os = "macos"))]
    {
        "gui/0".to_string()
    }
}

fn launchctl_user<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let rendered_args: Vec<String> = args
        .into_iter()
        .map(|entry| entry.as_ref().to_string())
        .collect();
    let output = launchctl_user_capture(rendered_args.iter().map(String::as_str))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let message = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        "<empty stdout/stderr>".to_string()
    };
    let status = output
        .status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| output.status.to_string());
    anyhow::bail!(
        "launchctl {} failed with status {}: {}",
        rendered_args.join(" "),
        status,
        message
    );
}

fn launchctl_user_capture<I, S>(args: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut command = Command::new("launchctl");
    let mut rendered_args = Vec::new();
    for arg in args {
        rendered_args.push(arg.as_ref().to_string());
        command.arg(arg.as_ref());
    }
    command_output_with_timeout(
        &mut command,
        Duration::from_secs(SYSTEMCTL_USER_TIMEOUT_SECS),
        &format!("launchctl {}", rendered_args.join(" ")),
    )
}

fn xml_escape_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

pub(super) fn systemctl_user<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let output = systemctl_user_capture(args)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let message = if !stderr.is_empty() { stderr } else { stdout };
    anyhow::bail!("systemctl --user failed: {message}");
}

pub(super) fn systemctl_user_capture<I, S>(args: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut command = Command::new("systemctl");
    command.arg("--user");
    configure_systemctl_user_env(&mut command);
    let mut rendered_args = vec!["--user".to_string()];
    for arg in args {
        rendered_args.push(arg.as_ref().to_string());
        command.arg(arg.as_ref());
    }
    command_output_with_timeout(
        &mut command,
        Duration::from_secs(SYSTEMCTL_USER_TIMEOUT_SECS),
        &format!("systemctl {}", rendered_args.join(" ")),
    )
}

pub(super) fn command_output_with_timeout(
    command: &mut Command,
    timeout: Duration,
    description: &str,
) -> Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to launch {description}"))?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if child
            .try_wait()
            .with_context(|| format!("failed to poll {description}"))?
            .is_some()
        {
            return child
                .wait_with_output()
                .with_context(|| format!("failed to collect {description} output"));
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let reap_deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < reap_deadline {
                if child
                    .try_wait()
                    .with_context(|| format!("failed to poll {description}"))?
                    .is_some()
                {
                    return child
                        .wait_with_output()
                        .with_context(|| format!("failed to collect {description} output"));
                }
                thread::sleep(Duration::from_millis(50));
            }
            anyhow::bail!("{description} timed out after {}s", timeout.as_secs());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn configure_systemctl_user_env(command: &mut Command) {
    #[cfg(unix)]
    {
        let runtime_dir = std::path::PathBuf::from(format!("/run/user/{}", unsafe { geteuid() }));
        if runtime_dir.is_dir() {
            command.env("XDG_RUNTIME_DIR", &runtime_dir);
            let bus_path = runtime_dir.join("bus");
            if bus_path.exists() {
                command.env(
                    "DBUS_SESSION_BUS_ADDRESS",
                    format!("unix:path={}", bus_path.display()),
                );
            }
        }
    }
}

pub(super) fn now_iso_string() -> String {
    chrono_like_iso(current_epoch_secs())
}

pub(super) fn current_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn chrono_like_iso(epoch_seconds: u64) -> String {
    use std::fmt::Write as _;

    let seconds_per_day = 86_400u64;
    let days = epoch_seconds / seconds_per_day;
    let seconds_of_day = epoch_seconds % seconds_per_day;

    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;

    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };

    let mut output = String::with_capacity(20);
    let _ = write!(
        output,
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    );
    output
}
