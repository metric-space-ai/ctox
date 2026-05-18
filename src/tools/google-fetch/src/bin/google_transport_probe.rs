use anyhow::{anyhow, bail, Context, Result};
use ctox_google_fetch::{
    fetch_google_response, response_markers, FetchRequest, FetchTransport, TransportProfile,
};
use serde::Serialize;
use std::env;

#[derive(Debug, Serialize)]
struct ProbeResult {
    profile: String,
    ok: bool,
    status: Option<u16>,
    final_url: Option<String>,
    len: Option<usize>,
    data_ved: Option<bool>,
    sorry: Option<bool>,
    captcha: Option<bool>,
    enablejs: Option<bool>,
    error: Option<String>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let request = parse_request_from_args()?;
    let mut results = Vec::new();
    for profile in TransportProfile::all() {
        match fetch_google_response(&request, profile) {
            Ok(response) => {
                let markers = response_markers(&response.body);
                results.push(ProbeResult {
                    profile: profile.label().to_string(),
                    ok: true,
                    status: Some(response.status),
                    final_url: Some(response.final_url),
                    len: Some(response.body.len()),
                    data_ved: Some(markers.data_ved),
                    sorry: Some(markers.sorry),
                    captcha: Some(markers.captcha),
                    enablejs: Some(markers.enablejs),
                    error: None,
                });
            }
            Err(err) => {
                results.push(ProbeResult {
                    profile: profile.label().to_string(),
                    ok: false,
                    status: None,
                    final_url: None,
                    len: None,
                    data_ved: None,
                    sorry: None,
                    captcha: None,
                    enablejs: None,
                    error: Some(format!("{err:#}")),
                });
            }
        }
    }

    serde_json::to_writer_pretty(std::io::stdout(), &results)
        .context("failed to encode Google transport probe output")?;
    println!();
    Ok(())
}

fn parse_request_from_args() -> Result<FetchRequest> {
    let mut args = env::args().skip(1);
    let mut url = None;
    let mut user_agent = None;
    let mut cookie_header = Some("CONSENT=YES+".to_string());
    let mut accept_language = Some("en,en-US;q=0.7,en;q=0.3".to_string());
    let mut timeout_ms = Some(30_000_u64);
    let mut emulation_major = Some(136_u16);

    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| anyhow!("missing value for argument `{flag}`"))?;
        match flag.as_str() {
            "--url" => url = Some(value),
            "--user-agent" => user_agent = Some(value),
            "--cookie" => cookie_header = Some(value),
            "--accept-language" => accept_language = Some(value),
            "--timeout-ms" => {
                timeout_ms = Some(
                    value
                        .parse::<u64>()
                        .with_context(|| format!("invalid --timeout-ms value `{value}`"))?,
                )
            }
            "--emulation-major" => {
                emulation_major = Some(
                    value
                        .parse::<u16>()
                        .with_context(|| format!("invalid --emulation-major value `{value}`"))?,
                )
            }
            _ => bail!("unknown argument `{flag}`"),
        }
    }

    Ok(FetchRequest {
        url: url.ok_or_else(|| anyhow!("missing required argument `--url`"))?,
        user_agent: user_agent
            .ok_or_else(|| anyhow!("missing required argument `--user-agent`"))?,
        cookie_header: cookie_header.expect("cookie default is set"),
        accept_language: accept_language.expect("accept-language default is set"),
        timeout_ms: timeout_ms.expect("timeout default is set"),
        emulation_major: emulation_major.expect("emulation major default is set"),
        transport: FetchTransport::Native,
        repo_root: None,
        extra_headers: Default::default(),
    })
}
