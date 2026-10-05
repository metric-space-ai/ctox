use std::io;
use std::process::{Command, Stdio};
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

pub(super) fn bounded_output(
    command: &mut Command,
    timeout: Duration,
) -> io::Result<std::process::Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output(),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "watchdog systemctl did not finish within its control budget",
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
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
