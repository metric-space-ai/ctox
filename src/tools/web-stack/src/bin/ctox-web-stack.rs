use anyhow::Context;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let root = take_value(&mut args, "--root")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir().context("failed to resolve current directory")?);
    let command = args.first().map(String::as_str).unwrap_or("");
    let value = match command {
        "browser-prepare" => ctox_web_stack::prepare_browser_environment(
            &root,
            &ctox_web_stack::BrowserPrepareOptions {
                dir: value(&args, "--dir").map(PathBuf::from),
                install_reference: args.iter().any(|arg| arg == "--install-reference"),
                install_browser: args.iter().any(|arg| arg == "--install-browser"),
                skip_npm_install: args.iter().any(|arg| arg == "--skip-npm-install"),
            },
        )?,
        "browser-automation" => ctox_web_stack::run_browser_automation(
            &root,
            &ctox_web_stack::BrowserAutomationRequest {
                dir: value(&args, "--dir").map(PathBuf::from),
                timeout_ms: value(&args, "--timeout-ms")
                    .map(str::parse)
                    .transpose()
                    .context("failed to parse --timeout-ms")?,
                source: automation_source(&args)?,
            },
        )?,
        "browser-capture" => ctox_web_stack::capture_browser_transport(
            &root,
            &ctox_web_stack::BrowserCaptureRequest {
                dir: value(&args, "--dir").map(PathBuf::from),
                out_dir: value(&args, "--out-dir").map(PathBuf::from),
                timeout_ms: value(&args, "--timeout-ms")
                    .map(str::parse)
                    .transpose()
                    .context("failed to parse --timeout-ms")?,
                url: value(&args, "--url")
                    .context("browser-capture requires --url <url>")?
                    .to_string(),
            },
        )?,
        _ => anyhow::bail!(
            "usage: ctox-web-stack [--root <path>] browser-prepare|browser-automation|browser-capture [...]"
        ),
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn take_value(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.remove(index);
    (index < args.len()).then(|| args.remove(index))
}

fn value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn automation_source(args: &[String]) -> anyhow::Result<String> {
    if let Some(path) = value(args, "--script-file") {
        return fs::read_to_string(path).with_context(|| format!("failed to read {path}"));
    }
    let mut source = String::new();
    std::io::stdin()
        .read_to_string(&mut source)
        .context("failed to read browser automation source")?;
    if source.trim().is_empty() {
        anyhow::bail!("browser-automation requires --script-file <path> or JavaScript on stdin");
    }
    Ok(source)
}
