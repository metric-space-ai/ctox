fn main() -> anyhow::Result<()> {
    ctox_process_network::dispatch()
        .unwrap_or_else(|| Err(anyhow::anyhow!("internal network helper only")))
}
