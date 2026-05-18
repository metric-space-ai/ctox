use ctox_pdf_parse::{
    load_sample_manifest, resolve_sample_pdf_path, resolve_sample_root, PdfSampleAsset,
};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Serialize)]
struct SampleSyncStatus {
    id: String,
    path: String,
    exists: bool,
    downloaded: bool,
    verified: Option<bool>,
    message: String,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let manifest_path = PathBuf::from(args.next().ok_or_else(|| {
        anyhow::anyhow!(
            "usage: fetch_public_samples <samples.json> [--root <dir>] [--sample <id>] [--check-only]"
        )
    })?);

    let mut root_override: Option<PathBuf> = None;
    let mut sample_filter: Option<String> = None;
    let mut check_only = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                root_override =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        anyhow::anyhow!("missing value for --root")
                    })?));
            }
            "--sample" => {
                sample_filter = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("missing value for --sample"))?,
                );
            }
            "--check-only" => check_only = true,
            _ => return Err(anyhow::anyhow!("unknown argument: {arg}")),
        }
    }

    let manifest = load_sample_manifest(&manifest_path)?;
    let sample_root = resolve_sample_root(&manifest_path, &manifest, root_override.as_deref());
    fs::create_dir_all(&sample_root)?;

    let mut statuses = Vec::new();
    for sample in &manifest.samples {
        if sample_filter
            .as_deref()
            .is_some_and(|filter| filter != sample.id)
        {
            continue;
        }

        statuses.push(sync_sample(
            &manifest_path,
            &manifest,
            sample,
            root_override.as_deref(),
            check_only,
        )?);
    }

    if statuses.is_empty() {
        return Err(anyhow::anyhow!(
            "no public samples matched the requested scope"
        ));
    }

    println!("{}", serde_json::to_string_pretty(&statuses)?);

    let all_ok = statuses
        .iter()
        .all(|status| status.exists && status.verified != Some(false));

    if all_ok {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "one or more public samples are missing or failed verification"
        ))
    }
}

fn sync_sample(
    manifest_path: &Path,
    manifest: &ctox_pdf_parse::PdfSampleManifest,
    sample: &PdfSampleAsset,
    root_override: Option<&Path>,
    check_only: bool,
) -> anyhow::Result<SampleSyncStatus> {
    let target_path = resolve_sample_pdf_path(manifest_path, manifest, sample, root_override);
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut downloaded = false;
    if !target_path.exists() && !check_only {
        download_sample(&sample.download_url, &target_path)?;
        downloaded = true;
    }

    let exists = target_path.exists();
    let verified = if exists {
        verify_checksum(&target_path, sample.sha256.as_deref())?
    } else {
        None
    };

    let message = if !exists {
        format!("missing; download from {}", sample.download_url)
    } else if downloaded && verified == Some(true) {
        "downloaded and verified".to_string()
    } else if downloaded {
        "downloaded".to_string()
    } else if verified == Some(true) {
        "present and verified".to_string()
    } else {
        "present".to_string()
    };

    Ok(SampleSyncStatus {
        id: sample.id.clone(),
        path: target_path.to_string_lossy().into_owned(),
        exists,
        downloaded,
        verified,
        message,
    })
}

fn download_sample(url: &str, target_path: &Path) -> anyhow::Result<()> {
    let tmp_path = target_path.with_extension("download");
    if tmp_path.exists() {
        fs::remove_file(&tmp_path)?;
    }

    if run_command(
        "curl",
        &[
            "-L",
            "--fail",
            "--silent",
            "--show-error",
            "-o",
            tmp_path.to_string_lossy().as_ref(),
            url,
        ],
    )? {
        fs::rename(tmp_path, target_path)?;
        return Ok(());
    }

    if run_python_download(url, &tmp_path)? {
        fs::rename(tmp_path, target_path)?;
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "failed to download {url}; neither curl nor python3 succeeded"
    ))
}

fn verify_checksum(path: &Path, expected_sha256: Option<&str>) -> anyhow::Result<Option<bool>> {
    let Some(expected_sha256) = expected_sha256 else {
        return Ok(None);
    };

    let Some(actual) = compute_sha256(path)? else {
        return Ok(None);
    };

    Ok(Some(actual.eq_ignore_ascii_case(expected_sha256)))
}

fn compute_sha256(path: &Path) -> anyhow::Result<Option<String>> {
    for (program, args) in [
        ("shasum", vec!["-a", "256", path.to_string_lossy().as_ref()]),
        ("sha256sum", vec![path.to_string_lossy().as_ref()]),
    ] {
        let output = match Command::new(program).args(&args).output() {
            Ok(output) => output,
            Err(_) => continue,
        };
        if !output.status.success() {
            continue;
        }

        let stdout = String::from_utf8(output.stdout)?;
        if let Some(hash) = stdout.split_whitespace().next() {
            return Ok(Some(hash.to_string()));
        }
    }

    Ok(None)
}

fn run_command(program: &str, args: &[&str]) -> anyhow::Result<bool> {
    let status = match Command::new(program).args(args).status() {
        Ok(status) => status,
        Err(_) => return Ok(false),
    };
    Ok(status.success())
}

fn run_python_download(url: &str, target_path: &Path) -> anyhow::Result<bool> {
    let status = match Command::new("python3")
        .args([
            "-c",
            "import sys, urllib.request; urllib.request.urlretrieve(sys.argv[1], sys.argv[2])",
            url,
            target_path.to_string_lossy().as_ref(),
        ])
        .status()
    {
        Ok(status) => status,
        Err(_) => return Ok(false),
    };

    Ok(status.success())
}
