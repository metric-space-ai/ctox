//! Renderers that consume the typed `Manuscript v1` and emit a deliverable.
//!
//! Render is a read-only operation on DB state. It refuses to run unless the
//! version it is asked to render has a passing `report_check_reports` row,
//! unless the operator passes `--force-no-check`.

pub mod docx;
pub mod md;

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use rusqlite::params;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;
use std::path::PathBuf;

use crate::report::check;
use crate::report::draft;
use crate::report::manuscript::Manuscript;
use crate::report::state_machine::{self, Status};
use crate::report::store;

#[derive(Debug, Clone, Serialize)]
pub struct RenderOutput {
    pub render_id: String,
    pub run_id: String,
    pub version_id: String,
    pub format: String,
    pub output_path: String,
    pub file_size_bytes: u64,
    pub sha256: String,
}

pub fn render(
    conn: &Connection,
    root: &Path,
    run_id: &str,
    version_id: Option<&str>,
    format: &str,
    out_path: Option<&Path>,
    force: bool,
) -> Result<RenderOutput> {
    let (version_id, _, _, manuscript) = draft::load_version(conn, run_id, version_id)?;
    if !force && !check::last_pass(conn, run_id, &version_id)? {
        bail!(
            "render refused: version {version_id} has no passing check report. \
             Run `ctox report check --run-id {run_id} --version-id {version_id}` first \
             (or pass --force-no-check explicitly to override)."
        );
    }
    let target_path = out_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| default_output_path(root, run_id, &version_id, format));
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create render dir {}", parent.display()))?;
    }
    let renderer_version = match format {
        "md" => {
            let bytes = md::render(&manuscript)?;
            std::fs::write(&target_path, &bytes)
                .with_context(|| format!("failed to write {}", target_path.display()))?;
            "ctox-report/md/v1".to_string()
        }
        "docx" => {
            let v = docx::render(&manuscript, &target_path)?;
            v
        }
        "json" => {
            let json_bytes = serde_json::to_vec_pretty(&manuscript)?;
            std::fs::write(&target_path, &json_bytes)?;
            "ctox-report/json/v1".to_string()
        }
        other => bail!("unsupported format '{other}' (supported: md, docx, json)"),
    };
    let bytes = std::fs::read(&target_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha = hex_lower(hasher.finalize().as_slice());
    let render_id = store::new_id("rnd");
    let now = store::now_iso();
    conn.execute(
        "INSERT INTO report_renders(render_id, run_id, version_id, format, output_path,
            file_size_bytes, sha256, renderer_version, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(run_id, version_id, format) DO UPDATE SET
            output_path = excluded.output_path,
            file_size_bytes = excluded.file_size_bytes,
            sha256 = excluded.sha256,
            renderer_version = excluded.renderer_version,
            created_at = excluded.created_at",
        params![
            render_id,
            run_id,
            version_id,
            format,
            target_path.to_string_lossy().to_string(),
            bytes.len() as i64,
            sha.clone(),
            renderer_version,
            now,
        ],
    )
    .context("failed to insert report_renders")?;
    state_machine::advance_to(conn, run_id, Status::Rendered).ok();
    Ok(RenderOutput {
        render_id,
        run_id: run_id.to_string(),
        version_id,
        format: format.to_string(),
        output_path: target_path.to_string_lossy().to_string(),
        file_size_bytes: bytes.len() as u64,
        sha256: sha,
    })
}

fn default_output_path(root: &Path, run_id: &str, version_id: &str, format: &str) -> PathBuf {
    let dir = root.join("runtime").join("reports").join(run_id);
    dir.join(format!("{version_id}.{format}"))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        s.push(char::from_digit(((byte >> 4) & 0xF) as u32, 16).unwrap());
        s.push(char::from_digit((byte & 0xF) as u32, 16).unwrap());
    }
    s
}

pub fn payload(out: &RenderOutput) -> Value {
    json!({
        "ok": true,
        "render_id": out.render_id,
        "version_id": out.version_id,
        "format": out.format,
        "output_path": out.output_path,
        "file_size_bytes": out.file_size_bytes,
        "sha256": out.sha256,
    })
}

pub fn export_manuscript(_conn: &Connection, manuscript: &Manuscript) -> Result<Value> {
    Ok(serde_json::to_value(manuscript)?)
}
