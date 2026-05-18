//! Thin subprocess wrapper around the systematic-research DOCX helpers.
//!
//! The bundled Python helper at
//! `skills/system/research/systematic-research/scripts/render_manuscript.py`
//! reads a manuscript JSON on stdin and writes a DOCX. This module
//! shells out to it. No timeout in this layer — the manager handles
//! overall timeouts.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::report::render::manuscript::Manuscript;

/// Successful render outcome; returned to the caller (the manager or
/// the `ctox report render` CLI in a later wave).
#[derive(Debug, Clone)]
pub struct DocxRenderOutcome {
    pub output_path: PathBuf,
    pub byte_count: u64,
    pub stdout_tail: String,
}

/// Failure modes exposed by [`render_docx`]. The manager picks the
/// `DependencyMissing` arm when the host needs a `pip install` hint
/// and `ScriptFailed` when the Python helper produced a non-zero exit
/// for any other reason.
#[derive(Debug)]
pub enum DocxRenderError {
    /// `python3` (or the configured executable) was not found on PATH.
    PythonNotFound,
    /// The helper script reported a missing Python dependency. The
    /// inner string names the dependency (e.g. `"python-docx"`).
    DependencyMissing(String),
    /// The helper script exited with a non-zero status that wasn't
    /// the dedicated dependency-missing code.
    ScriptFailed { exit_code: i32, stderr_tail: String },
    /// I/O error talking to the subprocess (spawn, stdin write, output
    /// read), or the helper script could not be located on disk.
    Io(std::io::Error),
}

impl std::fmt::Display for DocxRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocxRenderError::PythonNotFound => {
                write!(f, "python executable not found on PATH")
            }
            DocxRenderError::DependencyMissing(dep) => {
                write!(f, "python dependency missing: {dep}")
            }
            DocxRenderError::ScriptFailed {
                exit_code,
                stderr_tail,
            } => write!(
                f,
                "DOCX render helper exited with code {exit_code}: {stderr_tail}"
            ),
            DocxRenderError::Io(err) => write!(f, "DOCX render helper I/O error: {err}"),
        }
    }
}

impl std::error::Error for DocxRenderError {}

impl From<std::io::Error> for DocxRenderError {
    fn from(value: std::io::Error) -> Self {
        DocxRenderError::Io(value)
    }
}

/// Render a [`Manuscript`] to a DOCX file at `output_path` by invoking
/// the bundled Python helpers. `skill_root` must point at the directory
/// `skills/system/research/systematic-research`; the semantic renderer is
/// located at `<skill_root>/scripts/render_manuscript.py`. When present,
/// `<skill_root>/scripts/polish_docx_layout.py` is applied as a second pass
/// so normal CTOX renders produce a client-facing layout by default.
///
/// `python_executable` defaults to `"python3"` when `None`.
pub fn render_docx(
    manuscript: &Manuscript,
    output_path: &Path,
    skill_root: &Path,
    python_executable: Option<&str>,
) -> Result<DocxRenderOutcome, DocxRenderError> {
    let script_path = skill_root.join("scripts").join("render_manuscript.py");
    if !script_path.exists() {
        return Err(DocxRenderError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "render_manuscript.py not found at {}",
                script_path.display()
            ),
        )));
    }

    let payload = serde_json::to_vec(manuscript).map_err(|err| {
        DocxRenderError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("failed to serialise manuscript: {err}"),
        ))
    })?;

    let polish_script_path = skill_root.join("scripts").join("polish_docx_layout.py");
    let use_layout_polish = polish_script_path.exists();
    let render_output_path = if use_layout_polish {
        sibling_temp_path(output_path, "rendered")
    } else {
        output_path.to_path_buf()
    };

    let executable = python_executable.unwrap_or("python3");
    let mut command = Command::new(executable);
    command
        .arg(&script_path)
        .arg("--out")
        .arg(&render_output_path)
        .arg("--language")
        .arg(short_language(&manuscript.manifest.language))
        .arg("--report-type")
        .arg(&manuscript.manifest.report_type_id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(DocxRenderError::PythonNotFound);
        }
        Err(err) => return Err(DocxRenderError::Io(err)),
    };

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&payload)?;
        // Drop closes stdin so the script's `sys.stdin.read()` returns.
        drop(stdin);
    }

    let output = child.wait_with_output().map_err(DocxRenderError::Io)?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let exit_code = output.status.code().unwrap_or(-1);

    if !output.status.success() {
        if exit_code == 2 && stderr.to_ascii_lowercase().contains("python-docx") {
            return Err(DocxRenderError::DependencyMissing(
                "python-docx".to_string(),
            ));
        }
        return Err(DocxRenderError::ScriptFailed {
            exit_code,
            stderr_tail: tail(&stderr, 2000),
        });
    }

    let last_line = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();

    let (byte_count, parsed_path) = parse_ok_line(last_line).unwrap_or_else(|| {
        let fallback = fs::metadata(&render_output_path)
            .map(|m| m.len())
            .unwrap_or(0);
        (fallback, render_output_path.clone())
    });

    let mut stdout_tail = stdout;
    let (byte_count, parsed_path) = if use_layout_polish {
        let polish_output = run_layout_polisher(
            executable,
            &polish_script_path,
            &render_output_path,
            output_path,
        )?;
        let _ = fs::remove_file(&render_output_path);
        stdout_tail.push_str(&polish_output);
        let byte_count = fs::metadata(output_path)
            .map(|m| m.len())
            .unwrap_or(byte_count);
        (byte_count, output_path.to_path_buf())
    } else {
        (byte_count, parsed_path)
    };

    Ok(DocxRenderOutcome {
        output_path: parsed_path,
        byte_count,
        stdout_tail: tail(&stdout_tail, 2000),
    })
}

fn run_layout_polisher(
    executable: &str,
    polish_script_path: &Path,
    input_path: &Path,
    output_path: &Path,
) -> Result<String, DocxRenderError> {
    let output = Command::new(executable)
        .arg(polish_script_path)
        .arg("--in")
        .arg(input_path)
        .arg("--out")
        .arg(output_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                DocxRenderError::PythonNotFound
            } else {
                DocxRenderError::Io(err)
            }
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let exit_code = output.status.code().unwrap_or(-1);

    if !output.status.success() {
        if exit_code == 2 && stderr.to_ascii_lowercase().contains("python-docx") {
            return Err(DocxRenderError::DependencyMissing(
                "python-docx".to_string(),
            ));
        }
        return Err(DocxRenderError::ScriptFailed {
            exit_code,
            stderr_tail: tail(&stderr, 2000),
        });
    }

    Ok(format!("\n{stdout}"))
}

fn sibling_temp_path(output_path: &Path, label: &str) -> PathBuf {
    let file_name = output_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("report.docx");
    output_path.with_file_name(format!(".{file_name}.{label}.tmp.docx"))
}

/// Parse the script's `OK <byte_count> <output_path>` final line.
fn parse_ok_line(line: &str) -> Option<(u64, PathBuf)> {
    let mut tokens = line.split_whitespace();
    let head = tokens.next()?;
    if head != "OK" {
        return None;
    }
    let byte_count: u64 = tokens.next()?.parse().ok()?;
    let rest: String = tokens.collect::<Vec<_>>().join(" ");
    if rest.is_empty() {
        return None;
    }
    Some((byte_count, PathBuf::from(rest)))
}

/// Shorten a BCP-47 language tag to its primary subtag (`de-DE` -> `de`).
fn short_language(language: &str) -> String {
    language
        .split(|c| c == '-' || c == '_')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Trim a captured stream to the last `max_bytes` bytes (UTF-8 safe).
fn tail(stream: &str, max_bytes: usize) -> String {
    if stream.len() <= max_bytes {
        return stream.to_string();
    }
    let start = stream.len() - max_bytes;
    // Walk forward until we hit a UTF-8 boundary.
    let mut idx = start;
    while idx < stream.len() && !stream.is_char_boundary(idx) {
        idx += 1;
    }
    stream[idx..].to_string()
}
