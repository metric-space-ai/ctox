use super::*;
use ctox_network_proxy::{
    build_config_state, ConfigReloader, ConfigState, NetworkProxy, NetworkProxyConfig,
    NetworkProxyConstraints, NetworkProxyState,
};
use std::io::Write;
use std::sync::Arc;
struct Fixed(ConfigState);
#[async_trait::async_trait]
impl ConfigReloader for Fixed {
    fn source_label(&self) -> String {
        "process.run immutable per-call domains".into()
    }
    async fn maybe_reload(&self) -> Result<Option<ConfigState>> {
        Ok(None)
    }
    async fn reload_now(&self) -> Result<ConfigState> {
        Ok(self.0.clone())
    }
}
pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("host") if args.len() == 2 => host(&args[1]),
        Some("inside") if args.len() >= 4 && args[2] == "--" => inside(&args[1], &args[3..]),
        _ => bail!("invalid network helper arguments"),
    }
}
fn host(raw: &str) -> Result<()> {
    let domains: Vec<String> = serde_json::from_str(raw)?;
    if domains.is_empty() || domains.len() > 16 || domains.iter().any(|d| !super::domain(d)) {
        bail!("invalid network domains");
    }
    // This is a dedicated, single-threaded process. No ambient proxies,
    // reloader, policy prompt or app environment can broaden this policy.
    let mut config = NetworkProxyConfig::default();
    config.network.enabled = true;
    config.network.allowed_domains = domains;
    config.network.allow_upstream_proxy = false;
    config.network.enable_socks5 = false;
    config.network.enable_socks5_udp = false;
    let state = build_config_state(config, NetworkProxyConstraints::default())?;
    let state = Arc::new(NetworkProxyState::with_reloader(
        state.clone(),
        Arc::new(Fixed(state)),
    ));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let proxy = rt.block_on(NetworkProxy::builder().state(state).build())?;
    // Route preparation forks. It MUST precede proxy.run/DNS/task startup.
    for key in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
        std::env::set_var(key, format!("http://{}", proxy.http_addr()));
    }
    let spec = ctox_linux_sandbox::prepare_host_proxy_route_spec()?;
    rt.block_on(async {
        let _owner = proxy.run().await?;
        println!("{spec}");
        std::io::stdout().flush()?;
        std::future::pending::<Result<()>>().await
    })
}
fn inside(spec: &str, args: &[String]) -> Result<()> {
    use std::os::unix::process::CommandExt;
    // Override user/base proxy variables AFTER env_clear/envs at the parent.
    for key in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        std::env::set_var(key, "http://127.0.0.1:1");
    }
    for key in ["NO_PROXY", "no_proxy"] {
        std::env::set_var(key, "");
    }
    ctox_linux_sandbox::activate_proxy_routes_in_netns(spec)?;
    for (upper, lower) in [
        ("HTTP_PROXY", "http_proxy"),
        ("HTTPS_PROXY", "https_proxy"),
        ("ALL_PROXY", "all_proxy"),
    ] {
        std::env::set_var(lower, std::env::var(upper)?);
    }
    // Reuse the Harness filter AFTER bridge creation: the bridge workers
    // retain UDS access, while app code cannot open Unix/vsock/netlink sockets,
    // ptrace them, or bypass socket filtering through io_uring.
    ctox_linux_sandbox::apply_process_network_filter()?;
    Err(Command::new(&args[0]).args(&args[1..]).exec().into())
}
