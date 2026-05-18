use std::{thread, time::Duration};

use anyhow::Result;
use ctox_desktop::{
    connector::{RemoteSessionRequest, SessionKind},
    webrtc_terminal::WebRtcRemoteTerminal,
};

fn main() -> Result<()> {
    let terminal = WebRtcRemoteTerminal::connect(
        &RemoteSessionRequest {
            kind: SessionKind::Tui,
            signaling_urls: vec!["wss://api.metricspace.org/signal".to_owned()],
            auth_token: String::new(),
            password: "SuperDuper3GPU".to_owned(),
            room_id: "ctox-metricspace-3gpu".to_owned(),
            client_name: "desktop-debug".to_owned(),
            command_args: vec!["tui".to_owned()],
            title: "GPU3 debug".to_owned(),
        },
        40,
        140,
    )?;

    let mut last = String::new();
    for _ in 0..40 {
        thread::sleep(Duration::from_millis(500));
        let snapshot = terminal.snapshot();
        if snapshot.output != last {
            println!("--- snapshot ---");
            println!("{}", snapshot.output);
            last = snapshot.output;
        }
        if snapshot.exit_code.is_some() {
            break;
        }
    }

    terminal.close();
    Ok(())
}
