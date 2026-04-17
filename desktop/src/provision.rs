use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::Sender,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};

use crate::installations::{InstallChannel, RemoteAccessSettings, RemoteHostTarget, RemoteInstanceSource};

const REMOTE_INSTALL_URL: &str =
    "https://raw.githubusercontent.com/metric-space-ai/ctox/main/install.sh";

#[derive(Debug)]
pub enum ProvisionEvent {
    Status(String),
    Finished(Result<String, String>),
}

#[derive(Debug, Clone)]
pub struct ProvisionRequest {
    pub source_root: PathBuf,
    pub remote: RemoteAccessSettings,
}

pub fn run(request: ProvisionRequest, tx: Sender<ProvisionEvent>) {
    let result = run_inner(&request, &tx).map_err(|error| error.to_string());
    let _ = tx.send(ProvisionEvent::Finished(result));
}

fn run_inner(request: &ProvisionRequest, tx: &Sender<ProvisionEvent>) -> Result<String> {
    if request.remote.instance_source != RemoteInstanceSource::InstallNew {
        bail!("Provisioning is only available for 'Neue Instanz installieren'");
    }

    match request.remote.host_target {
        RemoteHostTarget::Localhost => provision_localhost(request, tx),
        RemoteHostTarget::Ssh => provision_ssh(request, tx),
        RemoteHostTarget::Unspecified => bail!("Please choose whether CTOX should run on this computer or another host"),
    }
}

fn provision_localhost(request: &ProvisionRequest, tx: &Sender<ProvisionEvent>) -> Result<String> {
    let install_script = request.source_root.join("install.sh");
    if install_script.is_file() {
        status(tx, "[1/2] Running CTOX installer (with GPU detection)...");
        run_streaming(
            Command::new("bash")
                .arg(&install_script)
                .arg("--rebuild")
                .arg(&request.source_root)
                .current_dir(&request.source_root),
            tx,
            "[1/2] Building CTOX via install.sh",
        )?;
    } else {
        status(tx, "[1/3] Preparing CTOX on this machine...");
        run_streaming(
            Command::new("cargo")
                .arg("build")
                .arg("--release")
                .arg("--bin")
                .arg("ctox")
                .current_dir(&request.source_root),
            tx,
            "[2/3] Building CTOX",
        )?;
    }
    run_streaming(
        Command::new("cargo")
            .arg("build")
            .arg("--release")
            .arg("--manifest-path")
            .arg("desktop/Cargo.toml")
            .arg("--bin")
            .arg("ctox-desktop-host")
            .current_dir(&request.source_root),
        tx,
        "Building desktop host",
    )?;

    Ok(format!(
        "Localhost vorbereitet.\nStartkommando:\n{}/desktop/target/release/ctox-desktop-host --root {} --signal {} --token <TOKEN> --password <PASSWORT> --room {} --name localhost",
        request.source_root.display(),
        request.source_root.display(),
        request.remote.signaling_urls.first().cloned().unwrap_or_default(),
        request.remote.room_id,
    ))
}

fn provision_ssh(request: &ProvisionRequest, tx: &Sender<ProvisionEvent>) -> Result<String> {
    let ssh_user = normalize_identity_token(&request.remote.ssh_user, "SSH user")?;
    let ssh_host = normalize_host_token(&request.remote.ssh_host)?;
    let ssh_password = request.remote.ssh_password.trim();
    if ssh_password.is_empty() {
        bail!("SSH password is required");
    }
    let ssh_port = request.remote.ssh_port;
    // Fast path: binary-first `curl | bash` on the remote. Only fall through
    // to the heavy source-upload flow when the user explicitly picks
    // `InstallChannel::LocalCheckout`.
    if request.remote.install_channel != InstallChannel::LocalCheckout {
        return provision_ssh_remote_installer(
            request,
            tx,
            &ssh_user,
            &ssh_host,
            ssh_port,
            ssh_password,
        );
    }
    let remote_target = format!("{ssh_user}@{ssh_host}");
    let remote_root = request.remote.install_root.trim();
    if remote_root.is_empty() {
        bail!("Please enter a target directory for the remote host");
    }
    let archive_name = format!(
        "ctox-desktop-{}.tar",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );
    let archive_path = std::env::temp_dir().join(&archive_name);
    let remote_archive = format!("/tmp/{archive_name}");

    // ── Step 1: Pack sources via git archive ──
    status(tx, "[1/5] Packing local sources (git archive)...");
    let git_result = Command::new("git")
        .arg("archive")
        .arg("--format=tar")
        .arg("HEAD")
        .arg("-o")
        .arg(&archive_path)
        .current_dir(&request.source_root)
        .output()
        .context("git archive failed")?;
    if !git_result.status.success() {
        // Fallback: plain tar
        let tar_result = Command::new("tar")
            .env("COPYFILE_DISABLE", "1")
            .arg("--exclude=target")
            .arg("--exclude=.git")
            .arg("-cf")
            .arg(&archive_path)
            .arg(".")
            .current_dir(&request.source_root)
            .output()
            .context("tar fallback failed")?;
        if !tar_result.status.success() {
            bail!("Failed to pack sources: {}", String::from_utf8_lossy(&tar_result.stderr).trim());
        }
    }
    let archive_size = std::fs::metadata(&archive_path)
        .map(|m| m.len() / (1024 * 1024))
        .unwrap_or(0);
    status(tx, &format!("[1/5] Source archive ready ({archive_size} MB)"));

    // ── Step 2: Upload via sshpass + scp ──
    status(tx, &format!("[2/5] Uploading {archive_size} MB to {ssh_host}..."));
    run_streaming(
        Command::new("sshpass")
            .arg("-p")
            .arg(ssh_password)
            .arg("scp")
            .arg("-o").arg("StrictHostKeyChecking=no")
            .arg("-P").arg(ssh_port.to_string())
            .arg(&archive_path)
            .arg(format!("{remote_target}:{remote_archive}")),
        tx,
        "[2/5] Uploading CTOX sources",
    )?;

    // ── Step 3: Extract on remote ──
    status(tx, "[3/5] Extracting sources on remote host...");
    run_ssh_cmd(
        ssh_password,
        &remote_target,
        ssh_port,
        &format!("mkdir -p {remote_root} && tar xf {remote_archive} -C {remote_root} && rm -f {remote_archive}"),
        tx,
        "[3/5] Extracting sources",
    )?;

    // ── Step 4: Build on remote (streaming via tail -f) ──
    // SSH buffers output in 4KB chunks, so we can't stream directly.
    // Instead: run build in background writing to a log file, then tail -f that file.
    let build_log = format!("{remote_root}/.ctox_build.log");
    status(tx, "[4/5] Building CTOX on remote machine (this may take 15-30 minutes on a fresh system)...");

    // Start build in background, writing all output to a log file
    let build_cmd = format!(
        "export PATH=$HOME/.cargo/bin:$PATH && \
         export CTOX_SUDO_PASSWORD='{sudo_password}' && \
         cd {remote_root} && \
         find . -name '._*' -delete 2>/dev/null; \
         ( \
           if ! command -v cargo >/dev/null 2>&1 && ! [ -x $HOME/.cargo/bin/cargo ]; then \
             echo 'Installing Rust toolchain...' && \
             curl --proto =https --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal && \
             export PATH=$HOME/.cargo/bin:$PATH; \
           fi && \
           if [ -f install.sh ]; then \
             bash install.sh --rebuild . ; \
           else \
             cargo build --release --bin ctox ; \
           fi && \
           if [ -f desktop/Cargo.toml ]; then \
             export PATH=$HOME/.cargo/bin:$PATH && \
             cargo build --release --manifest-path desktop/Cargo.toml --bin ctox-desktop-host ; \
           fi && \
           echo CTOX_BUILD_SUCCESS || echo CTOX_BUILD_FAILED \
         ) > {build_log} 2>&1 &",
        sudo_password = ssh_password,
    );

    // Start the build process in background
    run_ssh_cmd(
        ssh_password, &remote_target, ssh_port,
        &build_cmd,
        tx, "[4/5] Build started",
    )?;

    // Stream the build log via tail -f until we see SUCCESS or FAILED
    {
        let mut tail_child = Command::new("sshpass")
            .arg("-p").arg(ssh_password)
            .arg("ssh")
            .arg("-o").arg("StrictHostKeyChecking=no")
            .arg("-p").arg(ssh_port.to_string())
            .arg(&remote_target)
            .arg(format!("tail -n +1 -f {build_log}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to start tail -f for build log")?;

        let mut build_ok = false;
        if let Some(pipe) = tail_child.stdout.take() {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                let trimmed = line.trim().to_owned();
                if trimmed.contains("CTOX_BUILD_SUCCESS") {
                    build_ok = true;
                    break;
                }
                if trimmed.contains("CTOX_BUILD_FAILED") {
                    break;
                }
                if !trimmed.is_empty() {
                    let _ = tx.send(ProvisionEvent::Status(trimmed));
                }
            }
        }
        let _ = tail_child.kill();
        let _ = tail_child.wait();

        if !build_ok {
            // Read last lines of build log for error context
            let err_output = Command::new("sshpass")
                .arg("-p").arg(ssh_password)
                .arg("ssh")
                .arg("-o").arg("StrictHostKeyChecking=no")
                .arg("-p").arg(ssh_port.to_string())
                .arg(&remote_target)
                .arg(format!("tail -20 {build_log}"))
                .output()
                .ok();
            let detail = err_output
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                .unwrap_or_default();
            bail!("[4/5] Build failed on remote machine:\n{detail}");
        }
    }

    // Clean up local archive
    let _ = std::fs::remove_file(&archive_path);

    // ── Step 5: Start WebRTC host ──
    let signal_url = request.remote.signaling_urls.first().cloned().unwrap_or_default();
    let room = &request.remote.room_id;
    let password = &request.remote.password;
    let token = &request.remote.auth_token;

    status(tx, "[5/5] Starting WebRTC host on remote machine...");
    run_ssh_cmd(
        ssh_password,
        &remote_target,
        ssh_port,
        &format!(
            "cd {remote_root} && \
             pkill -x ctox-desktop-host 2>/dev/null; sleep 1; \
             nohup ./desktop/target/release/ctox-desktop-host \
               --root {remote_root} \
               --signal '{signal_url}' \
               --token '{token}' \
               --password '{password}' \
               --room '{room}' \
               --name '{ssh_host}' \
               > /tmp/ctox-desktop-host.log 2>&1 & disown; \
             sleep 2; \
             echo CTOX_HOST_STARTED"
        ),
        tx,
        "[5/5] Starting WebRTC host",
    )?;

    Ok(format!(
        "Remote-Host vorbereitet und WebRTC-Host gestartet: {remote_target}\n\
         Verbindung bereit via WebRTC (Signal: {signal_url})\n\
         Log: /tmp/ctox-desktop-host.log",
    ))
}

// ── Helper: run a local command, streaming stdout+stderr line-by-line ────────
fn run_streaming(command: &mut Command, tx: &Sender<ProvisionEvent>, label: &str) -> Result<()> {
    status(tx, label);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start {label}"))?;

    let stderr_pipe = child.stderr.take();
    let stdout_pipe = child.stdout.take();

    let tx_clone = tx.clone();
    let stderr_thread = std::thread::spawn(move || {
        if let Some(pipe) = stderr_pipe {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                let trimmed = line.trim().to_owned();
                if !trimmed.is_empty() {
                    let _ = tx_clone.send(ProvisionEvent::Status(trimmed));
                }
            }
        }
    });

    if let Some(pipe) = stdout_pipe {
        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
            let trimmed = line.trim().to_owned();
            if !trimmed.is_empty() {
                let _ = tx.send(ProvisionEvent::Status(trimmed));
            }
        }
    }

    let _ = stderr_thread.join();
    let exit_status = child.wait().with_context(|| format!("waiting for {label}"))?;
    if !exit_status.success() {
        bail!("{label} failed with exit status {exit_status}");
    }
    Ok(())
}

// ── Helper: run a single SSH command (non-streaming, for short ops) ──────────
fn run_ssh_cmd(
    password: &str,
    target: &str,
    port: u16,
    remote_cmd: &str,
    tx: &Sender<ProvisionEvent>,
    label: &str,
) -> Result<()> {
    status(tx, label);
    let output = Command::new("sshpass")
        .arg("-p")
        .arg(password)
        .arg("ssh")
        .arg("-o").arg("StrictHostKeyChecking=no")
        .arg("-p").arg(port.to_string())
        .arg(target)
        .arg(remote_cmd)
        .output()
        .with_context(|| format!("failed to run SSH command: {label}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() { &stderr } else { &stdout };
        if detail.is_empty() {
            bail!("{label} failed with exit status {}", output.status);
        }
        bail!("{label} failed: {detail}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !stdout.is_empty() {
        status(tx, &stdout);
    }
    Ok(())
}

// ── Helper: run SSH command with real-time output streaming ──────────────────
fn run_ssh_streaming(
    password: &str,
    target: &str,
    port: u16,
    remote_cmd: &str,
    tx: &Sender<ProvisionEvent>,
    label: &str,
) -> Result<()> {
    status(tx, label);
    let mut child = Command::new("sshpass")
        .arg("-p")
        .arg(password)
        .arg("ssh")
        .arg("-o").arg("StrictHostKeyChecking=no")
        .arg("-p").arg(port.to_string())
        .arg(target)
        .arg(remote_cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start SSH: {label}"))?;

    let stderr_pipe = child.stderr.take();
    let stdout_pipe = child.stdout.take();

    let tx_clone = tx.clone();
    let stderr_thread = std::thread::spawn(move || {
        if let Some(pipe) = stderr_pipe {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                let trimmed = line.trim().to_owned();
                if !trimmed.is_empty() {
                    let _ = tx_clone.send(ProvisionEvent::Status(trimmed));
                }
            }
        }
    });

    if let Some(pipe) = stdout_pipe {
        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
            let trimmed = line.trim().to_owned();
            if !trimmed.is_empty() {
                let _ = tx.send(ProvisionEvent::Status(trimmed));
            }
        }
    }

    let _ = stderr_thread.join();
    let exit_status = child.wait().with_context(|| format!("waiting for SSH: {label}"))?;
    if !exit_status.success() {
        bail!("{label} failed with exit status {exit_status}");
    }
    Ok(())
}

fn normalize_identity_token(value: &str, label: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{label} is required");
    }
    if trimmed
        .chars()
        .all(|char| char.is_ascii_alphanumeric() || matches!(char, '_' | '-' | '.'))
    {
        return Ok(trimmed.to_owned());
    }
    bail!("{label} contains unsupported characters")
}

fn normalize_host_token(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("Host/IP is required");
    }
    if trimmed.chars().all(|char| {
        char.is_ascii_alphanumeric() || matches!(char, '.' | '-' | ':' )
    }) {
        return Ok(trimmed.to_owned());
    }
    bail!("Host/IP contains unsupported characters")
}

fn status(tx: &Sender<ProvisionEvent>, message: &str) {
    let _ = tx.send(ProvisionEvent::Status(message.to_owned()));
}

/// Binary-first remote install: run `curl … | bash [flags]` on the remote via
/// SSH, streaming output back to the provisioning UI.
fn provision_ssh_remote_installer(
    request: &ProvisionRequest,
    tx: &Sender<ProvisionEvent>,
    ssh_user: &str,
    ssh_host: &str,
    ssh_port: u16,
    ssh_password: &str,
) -> Result<String> {
    let remote_target = format!("{ssh_user}@{ssh_host}");
    let installer_flags = match request.remote.install_channel {
        InstallChannel::Stable => "",
        InstallChannel::Dev => "-s -- --dev",
        InstallChannel::LocalCheckout => unreachable!("handled by caller"),
    };
    let label = match request.remote.install_channel {
        InstallChannel::Stable => "stable release",
        InstallChannel::Dev => "main branch (dev)",
        InstallChannel::LocalCheckout => unreachable!(),
    };

    status(tx, &format!("[1/3] Preparing remote install ({label})…"));

    // We pipe the installer to bash non-interactively. install.sh auto-
    // detects the backend; set CTOX_BACKEND in the remote shell profile if
    // you need to override.
    let remote_cmd = format!(
        "set -e; \
         curl -fsSL {url} | bash {flags}; \
         echo CTOX_INSTALL_SUCCESS",
        url = REMOTE_INSTALL_URL,
        flags = installer_flags,
    );

    status(
        tx,
        &format!("[2/3] Running installer on {remote_target} (may take several minutes)…"),
    );
    run_ssh_cmd(
        ssh_password,
        &remote_target,
        ssh_port,
        &remote_cmd,
        tx,
        "[2/3] Remote installer",
    )?;

    status(tx, "[3/3] Remote install finished.");
    Ok(format!(
        "Remote host {remote_target} is ready. CTOX was installed via {label}.\n\
         Next: configure the WebRTC signaling + credentials in the Instance \
         settings, then start `ctox-desktop-host --signal … --token … --password … --room …` on the host."
    ))
}

