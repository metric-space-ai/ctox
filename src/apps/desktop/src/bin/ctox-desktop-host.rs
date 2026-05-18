use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use ctox_desktop::webrtc_terminal::{run_host_bridge, HostBridgeConfig};

#[derive(Debug, Parser)]
#[command(name = "ctox-desktop-host")]
#[command(about = "Restricted CTOX WebRTC host for the CTOX desktop wrapper")]
struct Args {
    #[arg(long)]
    root: PathBuf,
    #[arg(long = "signal")]
    signaling_urls: Vec<String>,
    #[arg(long = "token")]
    auth_token: String,
    #[arg(long)]
    password: String,
    #[arg(long)]
    room: String,
    #[arg(long, default_value = "ctox-host")]
    name: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    run_host_bridge(HostBridgeConfig {
        root: args.root,
        signaling_urls: args.signaling_urls,
        auth_token: args.auth_token,
        password: args.password,
        room_id: args.room,
        host_name: args.name,
    })
}
