//! Jami account resolution and QR-code rendering for the settings
//! sidebar.
use super::*;

pub(super) fn render_qr_lines(payload: &str) -> Option<Vec<String>> {
    let code = QrCode::new(payload.as_bytes()).ok()?;
    let width = code.width();
    let colors = code.to_colors();
    let pad = 6usize;
    let mut matrix = vec![vec![false; width + pad * 2]; width + pad * 2];
    for y in 0..width {
        for x in 0..width {
            matrix[y + pad][x + pad] = matches!(colors[y * width + x], QrColor::Dark);
        }
    }
    if matrix.len() % 2 != 0 {
        matrix.push(vec![false; matrix[0].len()]);
    }
    let mut lines = Vec::with_capacity(matrix.len() / 2);
    for y in (0..matrix.len()).step_by(2) {
        let top = &matrix[y];
        let bottom = &matrix[y + 1];
        let mut line = String::with_capacity(top.len());
        for x in 0..top.len() {
            let ch = match (top[x], bottom[x]) {
                (false, false) => ' ',
                (true, false) => '▀',
                (false, true) => '▄',
                (true, true) => '█',
            };
            line.push(ch);
        }
        lines.push(line);
    }
    Some(lines)
}

pub(super) fn resolve_jami_runtime_account(
    root: &Path,
    configured_account_id: &str,
    configured_profile_name: &str,
) -> JamiResolveOutcome {
    let adapter = communication_adapters::jami();
    let resolved = adapter.resolve_account(
        root,
        &communication_adapters::JamiResolveAccountCommandRequest {
            account_id: Some(configured_account_id),
            profile_name: Some(configured_profile_name),
        },
    );
    let parsed = match resolved {
        Ok(value) => serde_json::from_value::<JamiResolvedEnvelope>(value),
        Err(err) => {
            return JamiResolveOutcome {
                account: None,
                error: Some(format!("failed to resolve jami adapter state: {err}")),
                dbus_env_file: None,
                checks: Vec::new(),
            };
        }
    };
    match parsed {
        Ok(parsed) => JamiResolveOutcome {
            account: parsed.resolved_account,
            error: if parsed.ok {
                parsed.error
            } else {
                parsed.error
            },
            dbus_env_file: parsed.dbus_env_file,
            checks: parsed.checks,
        },
        Err(err) => JamiResolveOutcome {
            account: None,
            error: Some(format!("failed to parse jami adapter output: {err}")),
            dbus_env_file: None,
            checks: Vec::new(),
        },
    }
}

pub(super) fn jami_missing_account_lines(
    dbus_env_file: Option<&str>,
    has_config: bool,
    checks: &[JamiDoctorCheck],
) -> Vec<String> {
    let mut lines = vec!["No live Jami RING account is available yet.".to_string()];
    if has_config {
        lines.push(
            "Configured account/profile could not be resolved to an active share URI.".to_string(),
        );
    } else {
        lines.push("No Jami account id or profile is configured yet, so the TUI cannot derive a QR target.".to_string());
    }
    if let Some(path) = dbus_env_file.filter(|value| !value.trim().is_empty()) {
        lines.push(format!("dbus {}", truncate_for_ui(path, 40)));
    }
    lines.extend(jami_doctor_hint_lines(checks));
    lines.push("Verify the Jami daemon is running and that a RING account exists.".to_string());
    lines
}

pub(super) fn jami_error_lines(
    error: &str,
    dbus_env_file: Option<&str>,
    has_config: bool,
    checks: &[JamiDoctorCheck],
) -> Vec<String> {
    let mut lines = vec!["Jami runtime is not ready.".to_string()];
    lines.push(format!("blocker {}", truncate_for_ui(error, 68)));
    if error.contains("DBUS_SESSION_BUS_ADDRESS") || error.contains("session bus") {
        lines.push(
            "Missing a user DBus session: start the Linux user bus or export DBUS_SESSION_BUS_ADDRESS before starting the Jami daemon."
                .to_string(),
        );
    }
    if let Some(path) = dbus_env_file.filter(|value| !value.trim().is_empty()) {
        lines.push(format!("dbus {}", truncate_for_ui(path, 40)));
    } else {
        lines.push("No Jami DBus env file is loaded yet.".to_string());
    }
    if has_config {
        lines.push(
            "Configured Jami account/profile is present, but runtime resolution still failed."
                .to_string(),
        );
    } else {
        lines.push("No configured Jami account/profile is available to fall back to.".to_string());
    }
    lines.extend(jami_doctor_hint_lines(checks));
    lines.push(
        "Start or repair the Jami daemon first; then reopen the Jami settings view.".to_string(),
    );
    lines
}

pub(super) fn jami_doctor_hint_lines(checks: &[JamiDoctorCheck]) -> Vec<String> {
    let mut lines = Vec::new();
    for check in checks.iter().filter(|check| !check.ok) {
        match check.name.as_str() {
            "automation_backend" => lines.push(
                "hint macOS Jami is not automatable through the current DBus adapter; use a manual share URI for QR or move Jami automation to a Linux runtime".to_string(),
            ),
            "dbus_env_file" => lines.push("hint start the CTOX Jami daemon so it writes CTO_JAMI_DBUS_ENV_FILE".to_string()),
            "dbus_session" => lines.push("hint ensure a Linux user DBus session is available before starting Jami".to_string()),
            "jami_runtime" => lines.push("hint brew install --cask jami".to_string()),
            "configured_identity" => lines.push("hint set CTO_JAMI_ACCOUNT_ID or CTO_JAMI_PROFILE_NAME".to_string()),
            _ => {}
        }
    }
    lines
}
