use std::io;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMER: &str = "ctox-watchdog.timer";
const SERVICE: &str = "ctox-watchdog.service";
type Runner = fn(&[&str]) -> io::Result<String>;
pub(super) type SystemWatchdogPause = WatchdogPause<Runner>;

/// Pause both sources of watchdog restarts: stopping just the timer leaves
/// an already dispatched oneshot free to start the daemon during cutover.
/// Unit refresh must not start the timer until this guard has been dropped.
pub(super) struct WatchdogPause<F: FnMut(&[&str]) -> io::Result<String>> {
    run: F,
    resume_timer: bool,
}

impl<F: FnMut(&[&str]) -> io::Result<String>> WatchdogPause<F> {
    fn acquire(mut run: F, installed: bool) -> io::Result<Self> {
        let mut resume_timer = false;
        if installed {
            let state = run(&["show", "--property=ActiveState", "--value", TIMER])?;
            resume_timer = matches!(state.trim(), "active" | "activating" | "reloading");
            run(&["stop", TIMER])?;
            if let Err(error) = run(&["stop", SERVICE]) {
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

pub(super) fn pause(installed: bool) -> io::Result<WatchdogPause<Runner>> {
    WatchdogPause::acquire(systemctl as Runner, installed)
}

fn systemctl(args: &[&str]) -> io::Result<String> {
    let mut command = Command::new("systemctl");
    command.arg("--user").args(args);
    let output = bounded_output(&mut command, Duration::from_secs(10))?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "systemctl --user {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn bounded_output(command: &mut Command, timeout: Duration) -> io::Result<std::process::Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output();
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
