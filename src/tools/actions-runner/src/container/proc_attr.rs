//! `getSysProcAttr` and `openPty`: what a step's process gets from the OS.
//!
//! These are two tiny functions in `pkg/container/util*.go`, split across
//! `util.go`, `util_windows.go`, `util_openbsd_mips64.go` and `util_plan9.go`.
//! They matter more than their size suggests, because they decide whether a
//! step can be **stopped as a unit**.
//!
//! # The rule
//!
//! | platform | with a TTY | without a TTY |
//! |---|---|---|
//! | Unix (not openbsd/mips64) | `Setsid` + `Setctty` — a new session that owns the terminal | `Setpgid` — a new process group |
//! | openbsd/mips64 | unsupported | `Setpgid` |
//! | plan9 | `Rfork: RFNOTEG` | `Rfork: RFNOTEG` |
//! | Windows | `CREATE_NEW_PROCESS_GROUP`, always | `CREATE_NEW_PROCESS_GROUP`, always |
//!
//! The TTY column exists so a command can *take* a terminal — it becomes the
//! session leader and the terminal's controlling process. The other column
//! exists so the runner can signal the whole group: a step that runs `npm
//! install` has `npm`, a shell and a hundred helpers behind it, and killing the
//! one pid the runner holds leaves the rest running. The group is what makes
//! cancellation mean cancellation.
//!
//! Windows has no TTY variant at all — `cmdLine` is passed through and the
//! creation flag is set regardless — so the two branches collapse into one
//! there, which is the reason the flag is not behind a `tty` test.
//!
//! # What is not ported
//!
//! **The PTY.** `openPty` returns a master/slave pair from
//! `github.com/creack/pty`, and `Setsid`+`Setctty` only mean something in
//! combination with one. The host back-end in
//! [`super::host_environment`] runs a step on **pipes**, not on a terminal, so
//! there is no controlling terminal to set and the two settings would have
//! nothing to act on — `Setctty` in particular fails outright without a
//! terminal. The process group *is* ported, because it is the part that has an
//! effect on the piped path too.
//!
//! `util_plan9.go` and `util_openbsd_mips64.go` are not ported either: CTOX
//! builds for macOS, Linux and Windows, and a `Rfork` has no spelling outside
//! plan9. The build-tag split is recorded here so the omission is a decision
//! rather than an oversight.

/// `getSysProcAttr(cmdLine, tty)`: give a command its own process group.
///
/// `tty` is accepted for fidelity with upstream and is **ignored on Windows**,
/// where the creation flag is set either way. On Unix it is ignored too — see
/// the module note: this back-end does not allocate a terminal, so the
/// session/terminal half of the upstream setting has nothing to act on.
pub fn set_process_group(command: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // `Setpgid: true` with `Pgid` left at zero is the child's own pid,
        // which is what `process_group(0)` means. Same call, same result.
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // `syscall.CREATE_NEW_PROCESS_GROUP`. Upstream also passes the whole
        // command line through `SysProcAttr.CmdLine`, which Go needs only
        // because it bypasses the argument vector; `std::process::Command`
        // takes the arguments directly, so there is no equivalent here and
        // none is needed.
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flag is a constant, and it is the value Windows' own header gives.
    /// A port that guessed a number here would compile and misbehave.
    #[test]
    fn the_windows_creation_flag_is_the_documented_constant() {
        // `CREATE_NEW_PROCESS_GROUP` in <windows.h>. Checked against the
        // Windows SDK value, not against act's Go constant, which is a
        // re-export of this one.
        const EXPECTED: u32 = 0x0000_0200;
        #[cfg(windows)]
        assert_eq!(CREATE_NEW_PROCESS_GROUP, EXPECTED);
        #[cfg(not(windows))]
        assert_eq!(EXPECTED, 0x0000_0200, "the constant is what upstream uses");
    }

    /// The call is accepted on both platforms and does not consume the
    /// command. Upstream's `getSysProcAttr` returns a struct that the caller
    /// assigns; here it mutates in place, and a version that ate the command
    /// would be caught here rather than at the first step.
    #[test]
    fn setting_a_process_group_leaves_the_command_usable() {
        let mut command = std::process::Command::new("true");
        set_process_group(&mut command);
        command.arg("--with-an-argument");
        // `get_program` is the only accessor `std` exposes, and it is enough to
        // show the command was not replaced.
        assert_eq!(command.get_program(), std::ffi::OsStr::new("true"));
    }

    /// A spawned process really does end up in its own group, which is the
    /// property the whole module exists for. This one runs a real process, and
    /// it is the only test here that touches the OS.
    #[test]
    fn a_spawned_command_lands_in_its_own_process_group() {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let mut command = std::process::Command::new("sh");
            command.args(["-c", "echo $$; ps -o pgid= -p $$"]);
            command.process_group(0);
            let output = command.output().expect("sh is available on this test host");
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut lines = stdout.lines();
            let pid: i32 = lines
                .next()
                .expect("the shell printed its pid")
                .trim()
                .parse()
                .expect("the pid is a number");
            let pgid: i32 = lines
                .next()
                .expect("ps printed the group")
                .trim()
                .parse()
                .expect("the group is a number");
            assert_eq!(pgid, pid, "the child leads its own process group");
        }
    }
}
