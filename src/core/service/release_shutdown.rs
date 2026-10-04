//! Release stopping never escalates a delayed shutdown to SIGKILL.
use anyhow::{Context, Result};
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) fn configure_systemd_stop(
    service_dir: &Path,
    timeout: Duration,
    mut control: impl FnMut(&[&str]) -> Result<String>,
) -> Result<()> {
    let dropins = service_dir.join("ctox.service.d");
    std::fs::create_dir_all(&dropins)?;
    let policy = format!(
        "[Service]\nTimeoutStopSec={}\nSendSIGKILL=no\n",
        timeout.as_secs()
    );
    std::fs::write(dropins.join("zz-ctox-release-stop.conf"), policy)?;
    control(&["daemon-reload"])?;
    let effective = control(&["show", "ctox.service", "--property=SendSIGKILL", "--value"])?;
    anyhow::ensure!(
        effective.trim() == "no",
        "refusing release stop: systemd would still escalate to SIGKILL"
    );
    Ok(())
}

/// All PIDs are selected by the caller's existing instance-root matcher.
/// ESRCH is already stopped; EPERM and any other signaling error refuse cutover.
#[cfg(unix)]
pub(super) fn stop_processes(pids: &[u32], deadline: Instant) -> Result<()> {
    for &pid in pids {
        anyhow::ensure!(
            pid > 1 && pid <= libc::pid_t::MAX as u32 && pid != std::process::id(),
            "unsafe release-stop PID"
        );
        let status = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        if status != 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::ESRCH) {
                return Err(err)
                    .with_context(|| format!("failed to request shutdown of PID {pid}"));
            }
        }
    }
    loop {
        let mut remaining = Vec::new();
        for &pid in pids {
            if unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
                remaining.push(pid);
            } else {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() != Some(libc::ESRCH) {
                    return Err(err)
                        .with_context(|| format!("cannot verify shutdown of PID {pid}"));
                }
            }
        }
        if remaining.is_empty() {
            return Ok(());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "refusing release switch: shutdown deadline left live PIDs {remaining:?}"
        );
        std::thread::sleep(
            Duration::from_millis(25).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_policy_is_reloaded_and_verified_before_stop() {
        let dir = tempfile::tempdir().unwrap();
        let mut calls = Vec::new();
        configure_systemd_stop(dir.path(), Duration::from_secs(315), |args| {
            calls.push(args.join(" "));
            Ok(if args[0] == "show" { "no\n" } else { "" }.to_owned())
        })
        .unwrap();
        assert_eq!(
            calls,
            [
                "daemon-reload",
                "show ctox.service --property=SendSIGKILL --value"
            ]
        );
        let content =
            std::fs::read_to_string(dir.path().join("ctox.service.d/zz-ctox-release-stop.conf"))
                .unwrap();
        assert!(content.contains("TimeoutStopSec=315\n"));
        assert!(content.contains("SendSIGKILL=no\n"));
    }

    #[test]
    fn effective_systemd_escalation_refuses_release_stop() {
        let dir = tempfile::tempdir().unwrap();
        let error = configure_systemd_stop(dir.path(), Duration::from_secs(315), |args| {
            Ok(if args[0] == "show" { "yes\n" } else { "" }.to_owned())
        })
        .unwrap_err();
        assert!(error.to_string().contains("would still escalate"));
    }

    #[cfg(unix)]
    struct OwnedProcess {
        pid: u32,
        thread: Option<std::thread::JoinHandle<std::process::ExitStatus>>,
    }
    #[cfg(unix)]
    impl Drop for OwnedProcess {
        fn drop(&mut self) {
            // Only this test-created process, after its production assertion.
            if let Some(thread) = self.thread.take() {
                if !thread.is_finished() {
                    unsafe {
                        libc::kill(self.pid as libc::pid_t, libc::SIGKILL);
                    }
                }
                let _ = thread.join();
            }
        }
    }

    #[cfg(unix)]
    fn witness(dir: &Path, ignores_term: bool) -> OwnedProcess {
        let ready = dir.join("ready");
        let script = if ignores_term {
            "import os,signal,sys,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);open(sys.argv[1],'w').close();time.sleep(30)"
        } else {
            "import os,signal,sys,time\ndef stop(*args):\n time.sleep(0.45)\n sys.exit(0)\nsignal.signal(signal.SIGTERM,stop)\nopen(sys.argv[1],'w').close()\ntime.sleep(30)"
        };
        let mut child = std::process::Command::new("python3")
            .args(["-c", script])
            .arg(&ready)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        let process = OwnedProcess {
            pid,
            thread: Some(std::thread::spawn(move || child.wait().unwrap())),
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() {
            assert!(
                Instant::now() < deadline,
                "owned witness readiness deadline"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        process
    }

    #[cfg(unix)]
    #[test]
    fn delayed_shutdown_outlives_old_200ms_kill_window() {
        let dir = tempfile::tempdir().unwrap();
        let mut process = witness(dir.path(), false);
        let start = Instant::now();
        stop_processes(&[process.pid], start + Duration::from_secs(3)).unwrap();
        assert!(start.elapsed() >= Duration::from_millis(400));
        assert!(process.thread.take().unwrap().join().unwrap().success());
    }

    #[cfg(unix)]
    #[test]
    fn shutdown_deadline_leaves_nonresponsive_process_alive_and_refuses_switch() {
        let dir = tempfile::tempdir().unwrap();
        let process = witness(dir.path(), true);
        let error = stop_processes(&[process.pid], Instant::now() + Duration::from_millis(150))
            .unwrap_err();
        assert!(error.to_string().contains("live PIDs"));
        assert_eq!(unsafe { libc::kill(process.pid as libc::pid_t, 0) }, 0);
    }
}
