use anyhow::{bail, Context, Result};
use ctox_google_fetch::{
    fetch_google_response, FetchRequest, FetchResponse, FetchTransport, TransportProfile,
};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .context("failed to read Google transport request from stdin")?;
    let request: FetchRequest =
        serde_json::from_str(&raw).context("failed to decode Google transport request")?;
    let wire = match request.transport {
        FetchTransport::Native => {
            let response = fetch_google_response(&request, TransportProfile::RustlsH2)?;
            FetchResponse {
                final_url: response.final_url,
                body: response.body,
            }
        }
        FetchTransport::BrowserClone => fetch_google_response_via_browser_clone(&request)?,
    };

    serde_json::to_writer(std::io::stdout(), &wire)
        .context("failed to encode Google transport response")?;
    Ok(())
}

fn fetch_google_response_via_browser_clone(request: &FetchRequest) -> Result<FetchResponse> {
    let repo_root = request
        .repo_root
        .as_ref()
        .context("Google browser transport requires `repo_root`")?;
    let script = PathBuf::from(repo_root).join("tools/google-fetch/browser_profile_probe.py");
    if !script.exists() {
        bail!(
            "Google browser transport probe is missing: {}",
            script.display()
        );
    }

    let output = Command::new("python3")
        .arg(&script)
        .arg("--full-clone")
        .arg("--quit-running-chrome")
        .arg("--emit-fetch-json")
        .arg("--url")
        .arg(&request.url)
        .current_dir(repo_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run Google browser transport probe")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        bail!("Google browser transport probe failed: {detail}");
    }

    serde_json::from_slice(&output.stdout)
        .context("failed to decode Google browser transport response")
}
