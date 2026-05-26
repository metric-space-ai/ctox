// Origin: CTOX
// License: AGPL-3.0-only

//! Stage-1 server binary entry point. Thin CLI on top of
//! `ctox_qwen36_35b_a3b_q4km_metal::server::run`.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use ctox_qwen36_35b_a3b_q4km_metal::server::{run, ServeConfig};

#[derive(Parser, Debug)]
#[command(
    name = "qwen36-35b-a3b-q4km-metal-server",
    about = "Local Unix-socket Responses-IPC server for the Qwen3.6-35B-A3B Q4_K_M Metal engine (stage-1 skeleton)"
)]
struct Args {
    /// Path the Unix-domain socket will be bound to (parent dir is
    /// chmod 0700, socket is chmod 0600).
    #[arg(long)]
    socket: PathBuf,
    /// Model id reported in `runtime_health.default_model`.
    #[arg(long, default_value = "Qwen/Qwen3.6-35B-A3B")]
    model_id: String,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();
    run(ServeConfig {
        socket: args.socket,
        model_id: args.model_id,
    })
    .await
}
