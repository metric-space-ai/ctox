//! Per-call network owner. The helper reuses the Harness proxy and UDS
//! routing, starts before daemon threads, and dies with its owning call.
#[cfg(target_os = "linux")]
use anyhow::Context;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Stdio;
use std::process::{Child, Command};

#[cfg(target_os = "linux")]
mod linux;

pub mod grants;

pub const HELPER: &str = "__ctox-process-network";
pub const SOCKET: &str = "/run/ctox-process-proxy.sock";

/// Held until the sandbox child and all output readers have finished.
pub struct Guard {
    child: Option<Child>,
    _directory: tempfile::TempDir,
    pub socket: PathBuf,
    pub spec: String,
}
impl Guard {
    pub fn start(executable: &Path, domains: &[String]) -> Result<Self> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (executable, domains);
            bail!("domain-filtered network requires Linux netns");
        }
        #[cfg(target_os = "linux")]
        {
            use std::io::{BufRead, BufReader};
            use std::os::unix::process::CommandExt;
            if domains.is_empty() || domains.len() > 16 || domains.iter().any(|d| !domain(d)) {
                bail!("invalid network target domains");
            }
            let directory = tempfile::Builder::new()
                .prefix("ctox-process-net-")
                .tempdir_in("/tmp")?;
            let mut command = Command::new(executable);
            command
                .args([HELPER, "host", &serde_json::to_string(domains)?])
                .env_clear()
                .env("TMPDIR", directory.path())
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .process_group(0);
            let parent = std::process::id() as libc::pid_t;
            unsafe {
                command.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0
                        || libc::getppid() != parent
                    {
                        return Err(std::io::Error::from_raw_os_error(libc::ECHILD));
                    }
                    Ok(())
                });
            }
            let child = command.spawn().context("start network owner")?;
            let mut guard = Self {
                child: Some(child),
                _directory: directory,
                socket: PathBuf::new(),
                spec: String::new(),
            };
            let stdout = guard
                .child
                .as_mut()
                .and_then(|child| child.stdout.take())
                .context("network owner stdout")?;
            let (tx, rx) = std::sync::mpsc::channel();
            let reader = std::thread::Builder::new()
                .spawn(move || {
                    let mut line = String::new();
                    let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
                    let _ = tx.send(result);
                })
                .context("start proxy readiness reader")?;
            let result = rx.recv_timeout(std::time::Duration::from_secs(10));
            if result.is_err() {
                guard.stop();
            }
            let _ = reader.join();
            let raw = result.context("network owner readiness timeout")??;
            let mut spec: serde_json::Value =
                serde_json::from_str(&raw).context("network owner failed readiness")?;
            let routes = spec["routes"]
                .as_array_mut()
                .context("missing proxy routes")?;
            if routes.is_empty() {
                bail!("missing proxy routes");
            }
            let socket = PathBuf::from(routes[0]["uds_path"].as_str().context("missing socket")?);
            if !socket.starts_with(guard._directory.path()) || !socket.exists() {
                bail!("invalid proxy socket");
            }
            for route in routes {
                if route["uds_path"].as_str() != socket.to_str() {
                    bail!("unexpected proxy route");
                }
                route["uds_path"] = SOCKET.into();
            }
            guard.socket = socket;
            guard.spec = serde_json::to_string(&spec)?;
            Ok(guard)
        }
    }
    fn stop(&mut self) {
        // Consume ownership once, and kill the group before reaping its PID.
        // This also terminates bridge workers if readiness failed.
        if let Some(mut child) = self.child.take() {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    pub fn wrap(&self, command: &mut Command, executable: &Path) {
        command
            .arg(executable)
            .args([HELPER, "inside", &self.spec, "--"]);
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Call before creating any daemon/runtime threads or opening stores.
pub fn dispatch() -> Option<Result<()>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some(HELPER) {
        return None;
    }
    #[cfg(target_os = "linux")]
    {
        Some(linux::run(&args[1..]))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Some(Err(anyhow::anyhow!("network helper requires Linux")))
    }
}
fn domain(name: &str) -> bool {
    name.len() <= 253
        && name.contains('.')
        && name.parse::<std::net::IpAddr>().is_err()
        && name.split('.').all(|s| {
            !s.is_empty()
                && s.len() <= 63
                && !s.starts_with('-')
                && !s.ends_with('-')
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
