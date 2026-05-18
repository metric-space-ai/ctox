use std::{collections::BTreeMap, path::PathBuf, thread, time::Duration};

use anyhow::Result;
use ctox_desktop::{
    connector::SessionSpec, installations::LaunchTarget, terminal_backend::TerminalSession,
};

fn main() -> Result<()> {
    let launch = LaunchTarget {
        program: "/Users/michaelwelsch/.local/lib/ctox/releases/v0.3.19/bin/ctox-real".to_owned(),
        args: vec!["tui".to_owned()],
        cwd: PathBuf::from("/Users/michaelwelsch/Documents/ctox.nosync"),
        env: BTreeMap::new(),
    };
    let terminal = TerminalSession::spawn(&SessionSpec::Local(launch), 40, 154)?;
    for tick in 0..20 {
        thread::sleep(Duration::from_millis(500));
        let snapshot = terminal.snapshot();
        println!(
            "--- snapshot {tick} alt={} exit={:?} len={} ---",
            snapshot.modes.alt_screen,
            snapshot.exit_code,
            snapshot.output.len()
        );
        println!("{}", snapshot.output);
        if snapshot.exit_code.is_some() {
            break;
        }
    }
    terminal.close();
    Ok(())
}
