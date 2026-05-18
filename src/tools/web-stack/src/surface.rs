use anyhow::Context;
use anyhow::Result;
use serde_json::{json, Value};
use std::path::Path;
use std::path::PathBuf;

use crate::browser::capture_browser_transport;
use crate::browser::prepare_browser_environment;
use crate::browser::read_browser_automation_source;
use crate::browser::run_browser_automation;
use crate::browser::BrowserAutomationRequest;
use crate::browser::BrowserCaptureRequest;
use crate::browser::BrowserPrepareOptions;
use crate::deep_research::run_ctox_deep_research_tool;
use crate::deep_research::DeepResearchDepth;
use crate::deep_research::DeepResearchRequest;
use crate::person_research::run_ctox_person_research_tool;
use crate::person_research::PersonResearchRequest;
use crate::scholarly_search::run_ctox_scholarly_search_tool;
use crate::scholarly_search::ScholarlySearchProvider;
use crate::scholarly_search::ScholarlySearchRequest;
use crate::sources::Country as SourceCountry;
use crate::sources::FieldKey as SourceFieldKey;
use crate::sources::ResearchMode as SourceResearchMode;
use crate::web_search::run_ctox_web_read_tool;
use crate::web_search::run_ctox_web_search_tool;
use crate::web_search::CanonicalWebSearchRequest;
use crate::web_search::ContextSize;
use crate::web_search::DirectWebReadRequest;
use crate::web_search::SearchUserLocation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebScrapeRequest {
    pub target_key: String,
    pub mode: WebScrapeMode,
    pub query: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebScrapeMode {
    Latest,
    Semantic,
}

impl WebScrapeRequest {
    pub fn forwarded_args(&self) -> Result<Vec<String>> {
        let mut forwarded = match self.mode {
            WebScrapeMode::Latest => {
                if self.query.is_some() {
                    anyhow::bail!(
                        "ctox web scrape --mode latest does not accept --query; use --mode semantic instead"
                    );
                }
                vec![
                    "show-latest".to_string(),
                    "--target-key".to_string(),
                    self.target_key.clone(),
                ]
            }
            WebScrapeMode::Semantic => {
                let query = self
                    .query
                    .as_ref()
                    .context("ctox web scrape --mode semantic requires --query <text>")?;
                vec![
                    "semantic-search".to_string(),
                    "--target-key".to_string(),
                    self.target_key.clone(),
                    "--query".to_string(),
                    query.clone(),
                ]
            }
        };
        if let Some(limit) = self.limit {
            forwarded.push("--limit".to_string());
            forwarded.push(limit.to_string());
        }
        Ok(forwarded)
    }
}

impl WebScrapeMode {
    fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "latest" => Ok(Self::Latest),
            "semantic" => Ok(Self::Semantic),
            other => anyhow::bail!("unsupported --mode `{other}`; expected `latest` or `semantic`"),
        }
    }
}

pub fn handle_web_command(
    root: &Path,
    args: &[String],
    scrape_executor: &dyn Fn(&Path, &[String]) -> Result<()>,
) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    if matches!(command, "" | "help" | "-h" | "--help") {
        println!("{}", web_usage());
        return Ok(());
    }
    match command {
        "search" => {
            if args
                .iter()
                .any(|arg| matches!(arg.as_str(), "help" | "-h" | "--help"))
            {
                println!("{}", web_usage());
                return Ok(());
            }
            let query = required_flag_value(args, "--query")
                .or_else(|| args.get(1).map(String::as_str))
                .context(
                    "usage: ctox web search --query <text> [--domain <host>]... [--source <id>]... [--country <DE|AT|CH>] [--context-size <low|medium|high>] [--cached] [--include-sources]",
                )?;
            let search_context_size = find_flag_value(args, "--context-size")
                .map(parse_context_size)
                .transpose()?;
            let user_location = SearchUserLocation {
                country: find_flag_value(args, "--country").map(|raw| raw.trim().to_string()),
                ..SearchUserLocation::default()
            };
            let payload = run_ctox_web_search_tool(
                root,
                &CanonicalWebSearchRequest {
                    query: query.to_string(),
                    external_web_access: args.iter().any(|arg| arg == "--cached").then_some(false),
                    allowed_domains: find_flag_values(args, "--domain")
                        .into_iter()
                        .map(ToOwned::to_owned)
                        .collect(),
                    user_location,
                    search_context_size,
                    search_content_types: Vec::new(),
                    include_sources: args.iter().any(|arg| arg == "--include-sources"),
                    pinned_sources: find_flag_values(args, "--source")
                        .into_iter()
                        .map(ToOwned::to_owned)
                        .collect(),
                },
            )?;
            print_json(&payload)
        }
        "read" => {
            if args
                .iter()
                .any(|arg| matches!(arg.as_str(), "help" | "-h" | "--help"))
            {
                println!("{}", web_usage());
                return Ok(());
            }
            let url = required_flag_value(args, "--url")
                .or_else(|| args.get(1).map(String::as_str))
                .context("usage: ctox web read --url <url> [--query <text>] [--find <text>]... [--country <DE|AT|CH>]")?;
            let payload = run_ctox_web_read_tool(
                root,
                &DirectWebReadRequest {
                    url: url.to_string(),
                    query: find_flag_value(args, "--query").map(ToOwned::to_owned),
                    find: find_flag_values(args, "--find")
                        .into_iter()
                        .map(ToOwned::to_owned)
                        .collect(),
                    country: find_flag_value(args, "--country").map(|s| s.trim().to_string()),
                },
            )?;
            print_json(&payload)
        }
        "scholarly" => handle_scholarly_command(root, &args[1..]),
        "sources" => handle_sources_command(&args[1..]),
        "person-research" => {
            let company = required_flag_value(args, "--company")
                .or_else(|| args.get(1).map(String::as_str))
                .context(
                    "usage: ctox web person-research --company <name> --country <DE|AT|CH> --mode <new_record|update_firm|update_person|update_inventory_general|have_data> [--field <field-key>]... [--include-private <source-id>]... [--workspace <path>] [--no-workspace]",
                )?;
            let country_raw = required_flag_value(args, "--country")
                .context("ctox web person-research requires --country <DE|AT|CH>")?;
            let country = SourceCountry::from_iso(country_raw)
                .with_context(|| format!("unsupported --country `{country_raw}`"))?;
            let mode_raw = required_flag_value(args, "--mode")
                .context("ctox web person-research requires --mode <new_record|update_firm|update_person|update_inventory_general|have_data>")?;
            let mode = SourceResearchMode::from_str(mode_raw)
                .with_context(|| format!("unsupported --mode `{mode_raw}`"))?;
            let fields: Vec<SourceFieldKey> = find_flag_values(args, "--field")
                .into_iter()
                .filter_map(SourceFieldKey::from_str)
                .collect();
            let include_private: Vec<String> = find_flag_values(args, "--include-private")
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let workspace = find_flag_value(args, "--workspace").map(PathBuf::from);
            let persist_workspace = !args.iter().any(|arg| arg == "--no-workspace");
            let payload = run_ctox_person_research_tool(
                root,
                &PersonResearchRequest {
                    company: company.to_string(),
                    country,
                    mode,
                    fields,
                    include_private,
                    workspace,
                    persist_workspace,
                },
            )?;
            print_json(&payload)
        }
        "deep-research" => {
            let query = required_flag_value(args, "--query")
                .or_else(|| args.get(1).map(String::as_str))
                .context(
                    "usage: ctox web deep-research --query <text> [--focus <text>] [--depth <quick|standard|exhaustive>] [--max-sources <n>] [--workspace <path>] [--include-annas-archive] [--no-papers] [--no-workspace]",
                )?;
            let depth = find_flag_value(args, "--depth")
                .map(parse_deep_research_depth)
                .transpose()?
                .unwrap_or_default();
            let max_sources = find_flag_value(args, "--max-sources")
                .map(|value| value.parse::<usize>())
                .transpose()
                .context("failed to parse --max-sources")?
                .unwrap_or(16);
            let payload = run_ctox_deep_research_tool(
                root,
                &DeepResearchRequest {
                    query: query.to_string(),
                    focus: find_flag_value(args, "--focus").map(ToOwned::to_owned),
                    depth,
                    max_sources,
                    include_annas_archive: args.iter().any(|arg| arg == "--include-annas-archive"),
                    include_papers: !args.iter().any(|arg| arg == "--no-papers"),
                    workspace: find_flag_value(args, "--workspace").map(PathBuf::from),
                    persist_workspace: !args.iter().any(|arg| arg == "--no-workspace"),
                },
            )?;
            print_json(&payload)
        }
        "scrape" => {
            let target_key = required_flag_value(args, "--target-key")
                .or_else(|| args.get(1).map(String::as_str))
                .context(
                    "usage: ctox web scrape --target-key <key> --mode <latest|semantic> [--query <text>] [--limit <n>]",
                )?;
            let mode = required_flag_value(args, "--mode")
                .context(
                    "usage: ctox web scrape --target-key <key> --mode <latest|semantic> [--query <text>] [--limit <n>]",
                )
                .and_then(WebScrapeMode::parse)?;
            let limit = find_flag_value(args, "--limit")
                .map(|value| value.parse::<usize>())
                .transpose()
                .context("failed to parse --limit")?;
            let request = WebScrapeRequest {
                target_key: target_key.to_string(),
                mode,
                query: find_flag_value(args, "--query")
                    .or_else(|| find_flag_value(args, "-q"))
                    .map(ToOwned::to_owned),
                limit,
            };
            let forwarded = request.forwarded_args()?;
            scrape_executor(root, &forwarded)
        }
        "browser-prepare" => {
            let payload = prepare_browser_environment(
                root,
                &BrowserPrepareOptions {
                    dir: find_flag_value(args, "--dir").map(PathBuf::from),
                    install_reference: args.iter().any(|arg| arg == "--install-reference"),
                    install_browser: args.iter().any(|arg| arg == "--install-browser"),
                    skip_npm_install: args.iter().any(|arg| arg == "--skip-npm-install"),
                },
            )?;
            print_json(&payload)
        }
        "browser-automation" => {
            let script_file = find_flag_value(args, "--script-file").map(PathBuf::from);
            let payload = run_browser_automation(
                root,
                &BrowserAutomationRequest {
                    dir: find_flag_value(args, "--dir").map(PathBuf::from),
                    timeout_ms: find_flag_value(args, "--timeout-ms")
                        .map(|value| value.parse::<u64>())
                        .transpose()
                        .context("failed to parse --timeout-ms")?,
                    source: read_browser_automation_source(script_file.as_deref())?,
                },
            )?;
            print_json(&payload)
        }
        "browser-capture" => {
            let url = required_flag_value(args, "--url")
                .or_else(|| args.get(1).map(String::as_str))
                .context(
                    "usage: ctox web browser-capture --url <url> [--out-dir <path>] [--timeout-ms <n>]",
                )?;
            let payload = capture_browser_transport(
                root,
                &BrowserCaptureRequest {
                    dir: find_flag_value(args, "--dir").map(PathBuf::from),
                    out_dir: find_flag_value(args, "--out-dir").map(PathBuf::from),
                    timeout_ms: find_flag_value(args, "--timeout-ms")
                        .map(|value| value.parse::<u64>())
                        .transpose()
                        .context("failed to parse --timeout-ms")?,
                    url: url.to_string(),
                },
            )?;
            print_json(&payload)
        }
        "unlock" => crate::unlock::handle_unlock_command(root, &args[1..]),
        _ => anyhow::bail!("{}", web_usage()),
    }
}

fn handle_sources_command(args: &[String]) -> Result<()> {
    use crate::sources;
    let action = args.first().map(String::as_str).unwrap_or("");
    if matches!(action, "" | "help" | "-h" | "--help" | "list") {
        let country_filter =
            find_flag_value(args, "--country").and_then(|raw| sources::Country::from_iso(raw));
        let tier_filter: Vec<String> = find_flag_values(args, "--tier")
            .into_iter()
            .map(|s| s.trim().to_ascii_uppercase())
            .collect();
        let field_filter =
            find_flag_value(args, "--field").and_then(|raw| sources::FieldKey::from_str(raw));
        let mut entries: Vec<Value> = Vec::new();
        for module in sources::list() {
            if let Some(c) = country_filter {
                if !module.countries().contains(&c) {
                    continue;
                }
            }
            if !tier_filter.is_empty() {
                let tier = match module.tier() {
                    sources::Tier::P => "P",
                    sources::Tier::S => "S",
                    sources::Tier::C => "C",
                };
                if !tier_filter.iter().any(|t| t == tier) {
                    continue;
                }
            }
            if let Some(f) = field_filter {
                if !module.authoritative_for().contains(&f) {
                    continue;
                }
            }
            entries.push(source_manifest_json(module));
        }
        print_json(&json!({
            "ok": true,
            "tool": "ctox_web_sources_list",
            "sources": entries,
        }))?;
        return Ok(());
    }
    if action == "info" {
        let id = required_flag_value(args, "--id")
            .or_else(|| args.get(1).map(String::as_str))
            .context("usage: ctox web sources info --id <source-id>")?;
        let module = sources::find(id).with_context(|| format!("unknown source: {id}"))?;
        print_json(&json!({
            "ok": true,
            "tool": "ctox_web_sources_info",
            "source": source_manifest_json(module),
        }))?;
        return Ok(());
    }
    anyhow::bail!("{}", sources_usage())
}

fn source_manifest_json(module: &'static dyn crate::sources::SourceModule) -> Value {
    use crate::sources::Tier;
    let tier = match module.tier() {
        Tier::P => "P",
        Tier::S => "S",
        Tier::C => "C",
    };
    let countries: Vec<&'static str> = module.countries().iter().map(|c| c.as_iso()).collect();
    let fields: Vec<&'static str> = module
        .authoritative_for()
        .iter()
        .map(|f| f.as_str())
        .collect();
    json!({
        "id": module.id(),
        "aliases": module.aliases(),
        "tier": tier,
        "countries": countries,
        "authoritative_for": fields,
        "requires_credential": module.requires_credential(),
    })
}

fn sources_usage() -> &'static str {
    "usage:\n  ctox web sources list [--country <DE|AT|CH>] [--tier <P|S|C>]... [--field <field-key>]\n  ctox web sources info --id <source-id>"
}

fn handle_scholarly_command(root: &Path, args: &[String]) -> Result<()> {
    let action = args.first().map(String::as_str).unwrap_or("");
    if matches!(action, "" | "help" | "-h" | "--help") {
        println!("{}", scholarly_usage());
        return Ok(());
    }
    match action {
        "search" => {
            if args
                .iter()
                .any(|arg| matches!(arg.as_str(), "help" | "-h" | "--help"))
            {
                println!("{}", scholarly_usage());
                return Ok(());
            }
            let query = required_flag_value(args, "--query")
                .or_else(|| args.get(1).map(String::as_str))
                .context(scholarly_usage())?;
            let provider =
                find_flag_value(args, "--provider").map(ScholarlySearchProvider::from_label);
            let content_types: Vec<String> = find_flag_values(args, "--content-type")
                .into_iter()
                .map(ToOwned::to_owned)
                .collect();
            let languages: Vec<String> = find_flag_values(args, "--language")
                .into_iter()
                .map(ToOwned::to_owned)
                .collect();
            let extensions: Vec<String> = find_flag_values(args, "--ext")
                .into_iter()
                .map(ToOwned::to_owned)
                .collect();
            let sort = find_flag_value(args, "--sort").map(ToOwned::to_owned);
            let max_results = find_flag_value(args, "--max-results")
                .map(|raw| raw.parse::<usize>())
                .transpose()
                .context("failed to parse --max-results")?;
            let page = find_flag_value(args, "--page")
                .map(|raw| raw.parse::<usize>())
                .transpose()
                .context("failed to parse --page")?;
            let with_oa_pdf = args.iter().any(|arg| arg == "--with-oa-pdf");
            let only_doi = args.iter().any(|arg| arg == "--only-doi");
            let payload = run_ctox_scholarly_search_tool(
                root,
                &ScholarlySearchRequest {
                    query: query.to_string(),
                    provider,
                    content_types,
                    languages,
                    extensions,
                    sort,
                    max_results,
                    page,
                    with_oa_pdf,
                    only_doi,
                },
            )?;
            print_json(&payload)
        }
        _ => anyhow::bail!("{}", scholarly_usage()),
    }
}

fn web_usage() -> &'static str {
    "usage:\n  ctox web search --query <text> [--domain <host>]... [--source <id>]... [--country <DE|AT|CH>] [--context-size <low|medium|high>] [--cached] [--include-sources]\n  ctox web read --url <url> [--query <text>] [--find <text>]... [--country <DE|AT|CH>]\n  ctox web sources list [--country <DE|AT|CH>] [--tier <P|S|C>]... [--field <field-key>]\n  ctox web sources info --id <source-id>\n  ctox web person-research --company <name> --country <DE|AT|CH> --mode <new_record|update_firm|update_person|update_inventory_general|have_data> [--field <field-key>]... [--include-private <source-id>]... [--workspace <path>] [--no-workspace]\n  ctox web scholarly search --query <text> [--provider <annas_archive>] [--content-type <type>]... [--language <code>]... [--ext <pdf|epub|...>]... [--sort <newest|oldest|largest|smallest|newest_added|oldest_added|random>] [--max-results <n>] [--page <n>] [--with-oa-pdf] [--only-doi]\n  ctox web deep-research --query <text> [--focus <text>] [--depth <quick|standard|exhaustive>] [--max-sources <n>] [--workspace <path>] [--include-annas-archive] [--no-papers] [--no-workspace]\n  ctox web scrape --target-key <key> --mode <latest|semantic> [--query <text>] [--limit <n>]\n  ctox web browser-prepare [--dir <path>] [--install-reference] [--install-browser] [--skip-npm-install]\n  ctox web browser-automation [--dir <path>] [--timeout-ms <n>] [--script-file <path>] < script.js\n  ctox web browser-capture --url <url> [--dir <path>] [--out-dir <path>] [--timeout-ms <n>]\n  ctox web unlock <list-probes|list-vectors|baseline|history|add-vector|set-vector-status> [...]"
}

fn scholarly_usage() -> &'static str {
    "usage: ctox web scholarly search --query <text> [--provider <annas_archive>] [--content-type <type>]... [--language <code>]... [--ext <pdf|epub|...>]... [--sort <newest|oldest|largest|smallest|newest_added|oldest_added|random>] [--max-results <n>] [--page <n>] [--with-oa-pdf] [--only-doi]"
}

fn parse_context_size(raw: &str) -> Result<ContextSize> {
    ContextSize::from_label(raw).with_context(|| {
        format!("unsupported --context-size `{raw}`; expected low, medium, or high")
    })
}

fn parse_deep_research_depth(raw: &str) -> Result<DeepResearchDepth> {
    DeepResearchDepth::from_label(raw).with_context(|| {
        format!("unsupported --depth `{raw}`; expected quick, standard, or exhaustive")
    })
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn find_flag_values<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == flag {
            if let Some(value) = args.get(index + 1) {
                out.push(value.as_str());
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    out
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::WebScrapeMode;
    use super::WebScrapeRequest;

    #[test]
    fn scrape_request_requires_explicit_latest_mode() {
        let request = WebScrapeRequest {
            target_key: "jobs".to_string(),
            mode: WebScrapeMode::Latest,
            query: None,
            limit: Some(5),
        };
        assert_eq!(
            request.forwarded_args().expect("latest scrape args"),
            vec!["show-latest", "--target-key", "jobs", "--limit", "5",]
        );
    }

    #[test]
    fn scrape_request_requires_explicit_semantic_mode() {
        let request = WebScrapeRequest {
            target_key: "jobs".to_string(),
            mode: WebScrapeMode::Semantic,
            query: Some("rust".to_string()),
            limit: Some(3),
        };
        assert_eq!(
            request.forwarded_args().expect("semantic scrape args"),
            vec![
                "semantic-search",
                "--target-key",
                "jobs",
                "--query",
                "rust",
                "--limit",
                "3",
            ]
        );
    }

    #[test]
    fn latest_scrape_mode_rejects_query() {
        let request = WebScrapeRequest {
            target_key: "jobs".to_string(),
            mode: WebScrapeMode::Latest,
            query: Some("rust".to_string()),
            limit: None,
        };
        let err = request
            .forwarded_args()
            .expect_err("latest scrape should reject query");
        assert!(err.to_string().contains("--mode latest"));
    }
}
