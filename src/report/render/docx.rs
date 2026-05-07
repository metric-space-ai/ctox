//! DOCX renderer: writes the manuscript JSON to a Python helper bundled with
//! the deep-research skill, which uses python-docx to produce a .docx file.
//!
//! Why a Python helper: python-docx is the established repo convention for
//! DOCX writing (see `skills/packs/content/doc/`). We avoid Node/docx-js as
//! it would add a JS toolchain.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;

use crate::report::manuscript::Manuscript;

const HELPER_RELATIVE: &str = "skills/system/research/deep-research/scripts/render_manuscript.py";

pub fn render(manuscript: &Manuscript, output_path: &Path) -> Result<String> {
    let helper = locate_helper()?;
    let json_payload = serde_json::to_string(manuscript)?;
    let interpreter = pick_python_with_docx()?;
    let mut child = Command::new(&interpreter)
        .arg(&helper)
        .arg("--format")
        .arg("docx")
        .arg("--out")
        .arg(output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {interpreter} {}", helper.display()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(json_payload.as_bytes())
            .context("failed to write manuscript to renderer stdin")?;
    }
    let output = child
        .wait_with_output()
        .context("renderer subprocess failed to complete")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "DOCX renderer exited with {} — stderr:\n{stderr}",
            output.status
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(line) = stdout.lines().find(|l| l.starts_with("renderer_version=")) {
        return Ok(line.trim_start_matches("renderer_version=").to_string());
    }
    Ok("ctox-report/docx/python-docx".to_string())
}

fn pick_python_with_docx() -> Result<String> {
    if let Ok(env_path) = std::env::var("CTOX_REPORT_PYTHON") {
        if !env_path.is_empty() {
            return Ok(env_path);
        }
    }
    let candidates = [
        "python3",
        "/usr/bin/python3",
        "/opt/homebrew/bin/python3.13",
        "/opt/homebrew/bin/python3.12",
        "/opt/homebrew/bin/python3.11",
        "python3.13",
        "python3.12",
        "python3.11",
    ];
    for cand in candidates {
        let probe = Command::new(cand)
            .arg("-c")
            .arg("import docx")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Ok(status) = probe {
            if status.success() {
                return Ok(cand.to_string());
            }
        }
    }
    bail!(
        "no python interpreter with python-docx was found. \
         Install with `python3 -m pip install python-docx` \
         or set CTOX_REPORT_PYTHON to point at one."
    )
}

fn locate_helper() -> Result<std::path::PathBuf> {
    // Search relative to common workspace roots so the renderer works in
    // dev (cwd = repo root) and in installed deployments
    // (cwd != repo root). Operator can override with CTOX_REPO_ROOT.
    if let Ok(env_path) = std::env::var("CTOX_REPO_ROOT") {
        let p = std::path::PathBuf::from(env_path).join(HELPER_RELATIVE);
        if p.is_file() {
            return Ok(p);
        }
    }
    let candidates = [
        std::env::current_dir()
            .ok()
            .map(|p| p.join(HELPER_RELATIVE)),
        Some(std::path::PathBuf::from(HELPER_RELATIVE)),
        std::env::var_os("HOME").map(|h| {
            std::path::PathBuf::from(h)
                .join(".local/lib/ctox")
                .join(HELPER_RELATIVE)
        }),
    ];
    for candidate in candidates.into_iter().flatten() {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!(
        "DOCX renderer helper not found. Set CTOX_REPO_ROOT or run from the repo root \
         (looked for {HELPER_RELATIVE})"
    );
}
