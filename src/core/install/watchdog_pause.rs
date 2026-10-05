use std::io;
#[cfg(unix)]
use std::process::{Command, Stdio};
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(test)]
const TIMER: &str = "ctox-watchdog.timer";
#[cfg(test)]
const SERVICE: &str = "ctox-watchdog.service";

/// Retain the former pause protocol's regression model. Production now holds
/// the release-switch flock; its systemctl calls use the bounded helper below.
#[cfg(test)]
struct WatchdogPause<F: FnMut(&[&str]) -> io::Result<String>> {
    run: F,
    resume_timer: bool,
}

#[cfg(test)]
impl<F: FnMut(&[&str]) -> io::Result<String>> WatchdogPause<F> {
    fn acquire(mut run: F, installed: bool) -> io::Result<Self> {
        let mut resume_timer = false;
        if installed {
            let state = run(&["show", "--property=ActiveState", "--value", TIMER])?;
            resume_timer = matches!(state.trim(), "active" | "activating" | "reloading");
            let stopped = run(&["stop", TIMER]).and_then(|_| run(&["stop", SERVICE]));
            if let Err(error) = stopped {
                if resume_timer {
                    if let Err(recovery) = run(&["start", TIMER]) {
                        eprintln!("ctox watchdog timer recovery failed: {recovery}");
                    }
                }
                return Err(error);
            }
        }
        Ok(Self { run, resume_timer })
    }
}

#[cfg(test)]
impl<F: FnMut(&[&str]) -> io::Result<String>> Drop for WatchdogPause<F> {
    fn drop(&mut self) {
        // Restore only the previously active timer, also on errors/unwind.
        // An intentionally stopped watchdog must remain stopped.
        if self.resume_timer {
            if let Err(error) = (self.run)(&["start", TIMER]) {
                eprintln!("ctox watchdog timer could not resume after release switch: {error}");
            }
        }
    }
}

#[cfg(unix)]
pub(super) fn bounded_output(
    command: &mut Command,
    timeout: Duration,
) -> io::Result<std::process::Output> {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;

    // A private session cannot contain the daemon or unrelated callers.
    // The control process and its inherited descendants are ours to reap.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let group = child.id() as libc::pid_t;
    let mut stdout = child.stdout.take().expect("piped control stdout");
    let mut stderr = child.stderr.take().expect("piped control stderr");
    let deadline = Instant::now() + timeout;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut stdout_done = false;
    let mut stderr_done = false;
    let mut status = None;
    let mut group_stopped = false;
    const OUTPUT_LIMIT: usize = 256 * 1024;

    let result = (|| {
        for fd in [stdout.as_raw_fd(), stderr.as_raw_fd()] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags == -1
                || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
            {
                return Err(io::Error::last_os_error());
            }
        }
        loop {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "watchdog control process or output exceeded its deadline",
                ));
            }
            for (reader, bytes, done) in [
                (&mut stdout as &mut dyn Read, &mut out, &mut stdout_done),
                (&mut stderr as &mut dyn Read, &mut err, &mut stderr_done),
            ] {
                if *done {
                    continue;
                }
                let mut buffer = [0; 8192];
                loop {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "watchdog control output exceeded its deadline",
                        ));
                    }
                    match reader.read(&mut buffer) {
                        Ok(0) => {
                            *done = true;
                            break;
                        }
                        Ok(n) => {
                            if bytes.len() + n > OUTPUT_LIMIT {
                                return Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    "watchdog control output exceeded its byte budget",
                                ));
                            }
                            bytes.extend_from_slice(&buffer[..n]);
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) => return Err(error),
                    }
                }
            }
            if status.is_none() {
                status = child.try_wait()?;
            }
            if status.is_some() && !group_stopped {
                // A finished parent may leave a descendant holding a pipe.
                // Stop only this new session's control group before draining.
                if unsafe { libc::kill(-group, libc::SIGKILL) } == -1 {
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() != Some(libc::ESRCH) {
                        return Err(error);
                    }
                }
                group_stopped = true;
            }
            if let Some(status) = status {
                if stdout_done && stderr_done {
                    return Ok(std::process::Output {
                        status,
                        stdout: out,
                        stderr: err,
                    });
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    })();
    // Close readers first, including on errors; no blocking EOF drain.
    drop(stdout);
    drop(stderr);
    if !group_stopped {
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
        let _ = child.kill();
    }
    // Never turn an uncertain cleanup into a successful cutover receipt.
    let cleanup_deadline = Instant::now() + Duration::from_millis(200);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < cleanup_deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(None) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("watchdog control group {group} cleanup is unconfirmed"),
                ));
            }
            Err(error) => return Err(error),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[cfg(unix)]
    #[test]
    fn watchdog_control_output_preserves_exit_code_and_both_streams() {
        let output = bounded_output(
            Command::new("/bin/sh").args(["-c", "printf out; printf err >&2; exit 7"]),
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"err");
    }

    #[cfg(unix)]
    #[test]
    fn watchdog_control_timeout_reaps_its_owned_control_process() {
        let directory = tempfile::tempdir().unwrap();
        let pid_path = directory.path().join("control.pid");
        let error = bounded_output(
            Command::new("/bin/sh")
                .args([
                    "-c",
                    "printf '%s' \"$$\" > \"$1\"; exec sleep 10",
                    "control",
                ])
                .arg(&pid_path),
            Duration::from_millis(500),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        let pid = std::fs::read_to_string(pid_path)
            .unwrap()
            .parse::<libc::pid_t>()
            .unwrap();
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    }

    #[cfg(unix)]
    #[test]
    fn watchdog_control_parent_exit_does_not_wait_for_descendant_output() {
        struct Cleanup {
            pid_path: std::path::PathBuf,
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                if let Ok(text) = std::fs::read_to_string(&self.pid_path) {
                    if let Ok(pid) = text.trim().parse::<libc::pid_t>() {
                        unsafe {
                            libc::kill(pid, libc::SIGKILL);
                        }
                    }
                }
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let pid_path = directory.path().join("descendant.pid");
        let _cleanup = Cleanup {
            pid_path: pid_path.clone(),
        };
        let started = Instant::now();
        let output = bounded_output(
            Command::new("/bin/sh")
                .args([
                    "-c",
                    "sleep 10 & printf '%s' \"$!\" > \"$1\"; printf parent; exit 0",
                    "control",
                ])
                .arg(&pid_path),
            Duration::from_millis(500),
        )
        .unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(output.status.success());
        assert_eq!(output.stdout, b"parent");
        let pid = std::fs::read_to_string(&pid_path)
            .unwrap()
            .parse::<libc::pid_t>()
            .unwrap();
        let until = Instant::now() + Duration::from_secs(1);
        while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
        std::fs::remove_file(&pid_path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn watchdog_control_output_is_bounded_before_full_capture() {
        let error = bounded_output(
            Command::new("head").args(["-c", "300000", "/dev/zero"]),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    #[cfg(unix)]
    fn watchdog_control_command_timeout_is_bounded() {
        let started = Instant::now();
        let error =
            bounded_output(Command::new("sleep").arg("2"), Duration::from_millis(30)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn release_switch_pauses_dispatched_watchdog_until_activation_finishes() {
        let events = RefCell::new(Vec::new());
        {
            let _guard = WatchdogPause::acquire(
                |args| {
                    events.borrow_mut().push(args.join(" "));
                    Ok("active\n".to_owned())
                },
                true,
            )
            .unwrap();
            events.borrow_mut().extend(
                [
                    "stop daemon",
                    "switch current",
                    "publish wrappers",
                    "start daemon",
                    "persist manifest",
                ]
                .map(str::to_owned),
            );
        }
        assert_eq!(
            *events.borrow(),
            [
                "show --property=ActiveState --value ctox-watchdog.timer",
                "stop ctox-watchdog.timer",
                "stop ctox-watchdog.service",
                "stop daemon",
                "switch current",
                "publish wrappers",
                "start daemon",
                "persist manifest",
                "start ctox-watchdog.timer",
            ]
        );
    }

    #[test]
    fn release_switch_restores_watchdog_on_failed_stop_without_activating_release() {
        let events = RefCell::new(Vec::new());
        let outcome: io::Result<()> = (|| {
            let _guard = WatchdogPause::acquire(
                |args| {
                    events.borrow_mut().push(args.join(" "));
                    Ok("active".to_owned())
                },
                true,
            )?;
            Err(io::Error::other("daemon still alive"))
        })();
        assert!(outcome.is_err());
        assert_eq!(events.borrow().last().unwrap(), "start ctox-watchdog.timer");
    }

    #[test]
    fn release_switch_refuses_cutover_when_running_watchdog_cannot_stop() {
        let events = RefCell::new(Vec::new());
        let guard = WatchdogPause::acquire(
            |args| {
                events.borrow_mut().push(args.join(" "));
                if args == ["stop", SERVICE] {
                    return Err(io::Error::other("oneshot cannot stop"));
                }
                Ok("active".to_owned())
            },
            true,
        );
        assert!(guard.is_err());
        assert_eq!(events.borrow().last().unwrap(), "start ctox-watchdog.timer");
    }

    #[test]
    fn release_switch_restores_timer_after_uncertain_timer_stop() {
        let events = RefCell::new(Vec::new());
        let guard = WatchdogPause::acquire(
            |args| {
                events.borrow_mut().push(args.join(" "));
                if args == ["stop", TIMER] {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "stop timed out"));
                }
                Ok("active".to_owned())
            },
            true,
        );
        assert!(guard.is_err());
        assert_eq!(
            *events.borrow(),
            [
                "show --property=ActiveState --value ctox-watchdog.timer",
                "stop ctox-watchdog.timer",
                "start ctox-watchdog.timer"
            ]
        );
    }

    #[test]
    fn release_switch_preserves_disabled_or_absent_watchdog() {
        for installed in [true, false] {
            let events = RefCell::new(Vec::new());
            drop(
                WatchdogPause::acquire(
                    |args| {
                        events.borrow_mut().push(args.join(" "));
                        Ok("inactive".to_owned())
                    },
                    installed,
                )
                .unwrap(),
            );
            assert!(!events
                .borrow()
                .iter()
                .any(|entry| entry.starts_with("start ")));
            assert_eq!(events.borrow().len(), if installed { 3 } else { 0 });
        }
    }
}
