#![recursion_limit = "256"]

use anyhow::Context;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

const APPSEC_PIPELINE_STAGE_MAX_ATTEMPTS: u64 = 3;
const APPSEC_PIPELINE_STAGE_RETRY_DELAY_SECONDS: i64 = 30;
const APPSEC_PIPELINE_RETRY_VERSION: &str = "ctox.appsec.pipeline_retry.v1";

mod api_costs;
mod appsec_state;
mod autonomy;
mod business_os;
mod capabilities;
mod coding_agents;
mod communication;
mod context;
mod doc_stack;
mod execution;
mod export;
mod install;
mod iot;
mod knowledge;
mod mission;
mod paths;
mod persistence;
mod report;
mod secrets;
mod service;
mod skill_store;
mod ui;
mod web_stack;

pub use capabilities::browser;
pub use capabilities::doc;
pub use capabilities::scrape;
pub use capabilities::web;
pub use context::context_health;
pub use context::context_stress;
pub use context::lcm;
pub use context::live_context;
pub use install::version_info;
pub use mission::channels;
pub use mission::follow_up;
pub use mission::plan;
pub use mission::queue;
pub use mission::review;
pub use mission::schedule;
pub use mission::strategy;
pub use mission::tickets;
pub use mission::verification;
pub use service::governance;
pub use service::mission_governor;
pub use service::state_invariants;
pub use ui::tui;

pub mod inference {
    pub use crate::execution::agent::turn_engine;
    pub use crate::execution::agent::turn_loop;
    pub use crate::execution::models::engine;
    pub use crate::execution::models::local_transport;
    pub use crate::execution::models::model_adapters;
    pub use crate::execution::models::model_manifest;
    pub use crate::execution::models::model_registry;
    pub use crate::execution::models::native_embedding;
    pub use crate::execution::models::native_stt;
    pub use crate::execution::models::native_tts;
    pub use crate::execution::models::resource_state;
    pub use crate::execution::models::runtime_contract;
    pub use crate::execution::models::runtime_control;
    pub use crate::execution::models::runtime_engine_guard;
    pub use crate::execution::models::runtime_env;
    pub use crate::execution::models::runtime_gpu_manager;
    pub use crate::execution::models::runtime_kernel;
    pub use crate::execution::models::runtime_plan;
    pub use crate::execution::models::runtime_state;
    pub use crate::execution::models::supervisor;
    pub use crate::execution::models::turn_contract;
    pub use crate::execution::responses::gateway;
    pub use crate::execution::responses::web_search;
}

use crate::inference::engine;
use crate::inference::gateway;
use crate::inference::model_registry;
use crate::inference::native_embedding;
use crate::inference::native_stt;
use crate::inference::native_tts;
use crate::inference::runtime_control;
use crate::inference::runtime_env;
use crate::inference::runtime_plan;
use crate::inference::runtime_state;

#[cfg(test)]
#[path = "model_catalog_boundary_tests.rs"]
mod model_catalog_boundary_tests;

fn print_help() {
    let version = env!("CTOX_BUILD_VERSION");
    println!(
        "ctox {version} — autonomous agent runtime

USAGE
  ctox [COMMAND] [OPTIONS]
  ctox                           open the TUI

EVERYDAY
  ctox start                     start the persistent mission loop (systemd service)
  ctox stop [--force]            stop the mission loop
  ctox status                    show service status (JSON)
  ctox work-hours set 08:00 18:00
                                 restrict CTOX work loop to local hours
  ctox work-hours off            disable working-hours restriction
  ctox chat <instruction>        submit a prompt to the running service
                                 add --wait to block until the slice completes
                                 add --to <addr> (repeatable), --cc <addr> (repeatable),
                                 and --subject <text> to mark the job as an
                                 owner/founder outbound email; the reply is
                                 routed through the reviewed-send pipeline
  ctox tui                       open the TUI
  ctox cost today|daily|week|month
                                 show API model costs by period
  ctox version                   print the version string

INSTALL / UPGRADE
  ctox upgrade [--stable|--dev]  one-shot update (default: latest release binary)
  ctox update check              poll the release channel for a new version
  ctox update apply --latest     same as `ctox upgrade --stable`
  ctox update apply --version <tag> [--from-source]
  ctox update apply --source <path> [--release <name>]
  ctox update rollback           revert to the previous release slot
  ctox update status             dump install layout + manifest + update state
  ctox business-os status        show bundled and native Business OS state
  ctox business-os rxdb status [--json]
                                 CTOX Sync Engine peer health: heartbeat, replicationUp,
                                 loop ticks, external-poll wakeups
  ctox business-os peer rotate   rotate the persisted Business OS WebRTC room
  ctox business-os serve [--addr 127.0.0.1:8765]
                                 serve the native no-build Business OS app
  ctox business-os desktop invite [--display-name <name>] [--format json|link]
                                 emit a Desktop pairing invite for this instance
  ctox business-os modules list|enable|disable
                                 manage optional Business OS skill-app modules
  ctox business-os app references [--query <text>] [--limit <n>|--all] [--json]
                                 list local Business OS apps an agent can inspect as references
  ctox business-os app references [--query <text>] [--limit <n>|--all] [--json]
                                 list local Business OS app reference candidates
  ctox business-os app create --instruction <text> [--module-id <id>]
                                 enqueue an agent-led runtime Business OS app creation task
  ctox business-os app modify <module-id> --instruction <text>
                                 enqueue an agent-led Business OS app modification task
  ctox business-os app validate <module-id> [--installed|--source]
                                 validate a Business OS app module artifact
  ctox business-os app smoke <module-id> [--url <business-os-url>]
                                 run a real browser smoke for a Business OS app module
  ctox business-os app e2e <module-id> [--url <business-os-url>]
                                 run save/reload/command-bus E2E for a Business OS app
  ctox business-os app bench run --suite core-five --model minimax-m3 --context 256k
                                 submit the five-app Business OS app creation bench
  ctox business-os skills list|enable|disable
                                 manage optional packed skills for Business OS
  ctox business-os repair queue-projections --dry-run|--apply
                                 reconcile durable CTOX queue state into Business OS projections
  ctox coding-agent status|providers|install|auth|workspace|session
                                 control desktop coding agents through a unified CLI

ENGINE / GPU
  ctox doctor                    health check — update available? hints

RUN / EXEC
  ctox runtime switch <model> <quality|performance> [--context 256k] [--timeout <secs>]
  ctox runtime embedding-doctor
  ctox runtime embedding-smoke [--token-id <id>]
  ctox runtime stt-doctor
  ctox runtime stt-smoke <wav-path>
  ctox runtime stt-realtime-smoke <wav-path>
  ctox runtime tts-doctor
  ctox runtime tts-smoke [--text <text>]
  ctox runtime openrouter-tool-smoke [--model <id>] [--tool-choice auto|required|named|all]
  ctox boost status|start|stop   temporary model/runtime boost lease

CAPABILITIES / WEB STACK
  ctox browser <subcmd>          interactive browser automation
  ctox web <subcmd>              web search/read tooling
  ctox appsec <subcmd>           deployment audit and go-live readiness workflow
  ctox scrape <subcmd>           scraping and extraction helpers
  ctox doc <subcmd>              document stack helpers
  ctox verification <subcmd>     verification records and evidence checks
  ctox skills <subcmd>           system/user skill catalog and pack management

GOVERNANCE / MISSION
  ctox service --foreground      run the daemon loop in the foreground
  ctox governance <subcmd>       governance decisions and audits
  ctox channel <subcmd>          communication channels (email, jami, webrtc)
  ctox mailserver <subcmd>       manage mailserver domains, users, and send test emails
  ctox queue <subcmd>            inspect, repair, and manage the service queue
  ctox report <subcmd>           deep research report runs (feasibility / market / decision brief / …)
  ctox plan <subcmd>             mission plans
  ctox schedule <subcmd>         recurring / deferred work
  ctox strategy <subcmd>         canonical vision / mission / strategic directives
  ctox follow-up <subcmd>        blocked-task follow-ups
  ctox ticket <subcmd>           ticket integrations
  ctox secret <subcmd>           credential storage
  ctox cost <subcmd>             API model token/cost accounting
  ctox state-invariants [--conversation-id <id>]
  ctox turn status|end           inspect or close the current CLI turn ledger
  ctox harness-flow              render the current harness work flow as ASCII
  ctox process-mining <subcmd>   SQLite mutation event log and transition mining
  ctox harness-mining <subcmd>   forensic + conformance mining of the agent harness
                                 (stuck-cases, variants, sojourn, conformance,
                                  alignment, causal, drift, multiperspective)
  ctox reset <target>            clear/rebuild the logging + process-mining audit
                                 trail (process-mining [--hard] | harness-mining |
                                 all); destructive runs require --confirm

CONTEXT / LCM (power-user)
  ctox lcm-init | lcm-add-message | lcm-compact | lcm-grep | lcm-dump
  ctox lcm-describe | lcm-expand | lcm-refresh-continuity | lcm-show-continuity
  ctox continuity-init | continuity-show | continuity-apply | continuity-log
  ctox continuity-forgotten | continuity-build-prompt | continuity-rebuild
  ctox context-health | context-retrieve | context-stress
  ctox chat-prompt-export

CONFIGURE THE UPDATE CHANNEL (for forks)
  ctox update channel show | clear
  ctox update channel set-github --repo <owner/repo> [--api-base <url>] [--token-env <env>]

ENVIRONMENT
  CTOX_ROOT, CTOX_STATE_ROOT, CTOX_INSTALL_ROOT, CTOX_CACHE_ROOT, CTOX_BIN_DIR
  CTOX_UPDATE_GITHUB_REPO / _API_BASE / _TOKEN_ENV   (override release channel)

Full reference: https://metric-space-ai.github.io/ctox/cli.html"
    );
}

/// Raise the soft open-file limit (RLIMIT_NOFILE) toward the hard limit at
/// process startup. The in-process Business OS native RxDB/WebRTC peer opens,
/// under a full sync, roughly one signaling socket + one WebRTC peer connection
/// + several SQLite file handles per collection across ~80 collections. The
/// default soft limit of 1024 is exhausted (EMFILE: "Too many open files"),
/// which makes SQLite fail to open databases, signaling sockets fail, and the
/// peer status heartbeat fail — collapsing the native peer so browsers can no
/// longer sync. Raising soft to the (already large) hard limit is unprivileged
/// and avoids depending on a per-host systemd `LimitNOFILE` override.
#[cfg(unix)]
fn raise_open_file_limit() {
    // SAFETY: plain libc getrlimit/setrlimit calls with a stack-local rlimit.
    unsafe {
        let mut rl = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut rl) == 0 && rl.rlim_cur < rl.rlim_max {
            let new = libc::rlimit {
                rlim_cur: rl.rlim_max,
                rlim_max: rl.rlim_max,
            };
            let _ = libc::setrlimit(libc::RLIMIT_NOFILE, &new);
        }
    }
}

#[cfg(not(unix))]
fn raise_open_file_limit() {}

fn main() -> anyhow::Result<()> {
    raise_open_file_limit();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = resolve_workspace_root()?;
    if args.first().map(String::as_str) == Some("__native-qwen3-embedding-service") {
        return handle_native_qwen3_embedding_service(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("__native-voxtral-stt-service") {
        return handle_native_voxtral_stt_service(&args[1..], &root);
    }
    if args.first().map(String::as_str) == Some("__native-voxtral-tts-service") {
        return handle_native_voxtral_tts_service(&args[1..], &root);
    }

    if skips_cli_startup_db(&args) {
        return dispatch_command(&root, &args);
    }

    service::db_migration::run_if_needed(&root)
        .context("failed to consolidate legacy databases into runtime/ctox.sqlite3")?;

    if skips_cli_turn_ledger(&args) {
        return dispatch_command(&root, &args);
    }

    let mut cli_ledger = service::turn_ledger::CliCommandLedger::start(&root, &args)
        .context("failed to start CTOX CLI turn ledger")?;
    let result = dispatch_command(&root, &args);
    cli_ledger
        .finish(&result)
        .context("failed to finish CTOX CLI turn ledger")?;
    result
}

fn skips_cli_startup_db(args: &[String]) -> bool {
    if args.is_empty() {
        return true;
    }
    if let Some(first) = args.first().map(String::as_str) {
        match first {
            "tui" | "tui-smoke" => return true,
            _ => {}
        }
    }
    skips_cli_turn_ledger(args)
}

fn skips_cli_turn_ledger(args: &[String]) -> bool {
    // Diagnostic and administrative commands MUST NOT block on the CLI
    // turn ledger. Opening the ledger requires an fcntl write lock on
    // runtime/ctox.sqlite3, and on macOS a previous CTOX CLI process
    // stuck in uninterruptible-exit (UE) state holds that lock until the
    // kernel reaps it — which never happens. Without this skip, a single
    // stuck `ctox` invocation poisons every subsequent `ctox upgrade`,
    // `ctox status`, etc. with an indefinite SQLite hang. The legacy
    // runtime-* smokes below were the original entries.
    if let Some(first) = args.first().map(String::as_str) {
        match first {
            // Recovery / inspection commands — must work even when the
            // runtime DB is wedged.
            "upgrade" | "update" | "version" | "status" | "doctor" | "mailserver" | "appsec" => {
                return true;
            }
            "business-os" | "business"
                if matches!(args.get(1).map(String::as_str), Some("serve" | "status")) =>
            {
                return true;
            }
            "business-os" | "business"
                if args.get(1).map(String::as_str) == Some("peer")
                    && matches!(
                        args.get(2).map(String::as_str),
                        None | Some("status" | "ensure" | "rotate")
                    ) =>
            {
                return true;
            }
            "business-os" | "business"
                if args.get(1).map(String::as_str) == Some("rxdb")
                    && matches!(
                        args.get(2).map(String::as_str),
                        None | Some("repair-optional-drift" | "help" | "--help" | "-h")
                    ) =>
            {
                return true;
            }
            "business-os" | "business"
                if args.get(1).map(String::as_str) == Some("files")
                    && matches!(
                        args.get(2).map(String::as_str),
                        Some("sync" | "sync-workspace")
                    ) =>
            {
                return true;
            }
            _ => {}
        }
    }
    matches!(
        args,
        [command, subcommand, ..]
            if command == "runtime"
                && matches!(
                    subcommand.as_str(),
                    "embedding-doctor"
                        | "embedding-smoke"
                        | "stt-doctor"
                        | "stt-smoke"
                        | "stt-realtime-smoke"
                        | "tts-doctor"
                        | "tts-smoke"
                        | "openrouter-tool-smoke"
                )
    )
}

fn dispatch_command(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        None => tui::run_tui(root),
        Some("help") | Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some("source-status") => {
            let outcome = engine::source_layout_status(&root)?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
            Ok(())
        }
        Some("clean-room-baseline-plan") => {
            let family = args
                .get(1)
                .map(String::as_str)
                .unwrap_or(model_registry::default_local_chat_family_selector())
                .parse()?;
            let prompt = if args.len() > 2 {
                args[2..].join(" ")
            } else {
                "Reply with CTOX_BASELINE_OK and nothing else.".to_string()
            };
            let plan = engine::build_clean_room_baseline_plan(&root, family, prompt);
            println!("{}", serde_json::to_string_pretty(&plan)?);
            Ok(())
        }
        Some("clean-room-rewrite-responses") => {
            let input_path = args
                .get(1)
                .context("usage: ctox clean-room-rewrite-responses <json-path>")?;
            let raw = std::fs::read(input_path)
                .with_context(|| format!("failed to read responses payload from {}", input_path))?;
            let rewritten = engine::rewrite_engine_responses_request(&raw)?;
            println!("{}", String::from_utf8_lossy(&rewritten));
            Ok(())
        }
        Some("runtime") => match args.get(1).map(String::as_str) {
            Some("embedding-doctor") => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&native_embedding::doctor_json(&root))?
                );
                Ok(())
            }
            Some("embedding-smoke") => {
                let token_id = native_embedding::parse_embedding_smoke_token_id(&args[2..])?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&native_embedding::embedding_smoke_json(
                        &root, token_id
                    ))?
                );
                Ok(())
            }
            Some("stt-doctor") => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&native_stt::doctor_json(&root))?
                );
                Ok(())
            }
            Some("stt-smoke") => {
                let audio_path = native_stt::parse_stt_smoke_audio_path(&args[2..])?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&native_stt::stt_smoke_json(&root, &audio_path))?
                );
                Ok(())
            }
            Some("stt-realtime-smoke") => {
                let audio_path = native_stt::parse_stt_smoke_audio_path(&args[2..])?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&native_stt::stt_realtime_smoke_json(
                        &root,
                        &audio_path
                    ))?
                );
                Ok(())
            }
            Some("tts-doctor") => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&native_tts::doctor_json(&root))?
                );
                Ok(())
            }
            Some("tts-smoke") => {
                let text = native_tts::parse_tts_smoke_text(&args[2..])?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&native_tts::tts_smoke_json(&root, &text))?
                );
                Ok(())
            }
            Some("openrouter-tool-smoke") => {
                let result = openrouter_tool_smoke_json(&root, &args[2..])?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
            Some("switch") => {
                let model = args
                    .get(2)
                    .context(
                        "usage: ctox runtime switch <model> <quality|performance> [--context 256k] [--timeout <secs>]",
                    )?;
                let preset = args
                    .get(3)
                    .context(
                        "usage: ctox runtime switch <model> <quality|performance> [--context 256k] [--timeout <secs>]",
                    )?;
                let context = find_flag_value(&args[4..], "--context");
                let timeout = find_flag_value(&args[4..], "--timeout");
                let outcome = runtime_control::execute_runtime_switch_with_context(
                    &root,
                    model,
                    Some(preset),
                    context,
                )?;
                if timeout.is_some() {
                    persist_runtime_turn_timeout(&root, timeout)?;
                }
                if let Some(plan) = runtime_plan::load_persisted_chat_runtime_plan(&root)? {
                    println!("{}", serde_json::to_string_pretty(&plan)?);
                } else {
                    let state = runtime_state::load_or_resolve_runtime_state(&root)?;
                    println!("{}", serde_json::to_string_pretty(&state)?);
                }
                eprintln!(
                    "ctox runtime switch requested model={} context={} timeout={} active_model={} phase={:?}",
                    model,
                    context.unwrap_or("default"),
                    timeout.unwrap_or("default"),
                    outcome.active_model,
                    outcome.phase
                );
                Ok(())
            }
            _ => anyhow::bail!(
                "usage: ctox runtime switch <model> <quality|performance> [--context 256k] [--timeout <secs>] | ctox runtime embedding-doctor | ctox runtime embedding-smoke [--token-id <id>] | ctox runtime stt-doctor | ctox runtime stt-smoke <wav-path> | ctox runtime stt-realtime-smoke <wav-path> | ctox runtime tts-doctor | ctox runtime tts-smoke [--text <text>] | ctox runtime openrouter-tool-smoke [--model <id>] [--tool-choice auto|required|named|all]"
            ),
        },
        Some("boost") => match args.get(1).map(String::as_str) {
            Some("status") => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&gateway::boost_status(&root)?)?
                );
                Ok(())
            }
            Some("start") => {
                let minutes = find_flag_value(&args[2..], "--minutes")
                    .and_then(|value| value.parse::<u64>().ok());
                let model = find_flag_value(&args[2..], "--model");
                let reason = find_flag_value(&args[2..], "--reason");
                let result = gateway::start_boost_lease(&root, model, minutes, reason)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
            Some("stop") => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&gateway::stop_boost_lease(&root)?)?
                );
                Ok(())
            }
            _ => anyhow::bail!(
                "usage: ctox boost status | ctox boost start [--minutes <n>] [--model <id>] [--reason <text>] | ctox boost stop"
            ),
        },
        Some("service") => {
            let flags: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
            let foreground = flags.contains(&"--foreground");
            if !foreground {
                anyhow::bail!(
                    "usage: ctox service --foreground \
                     [--autonomy progressive|balanced|defensive] \
                     [--auto-approve-gates (deprecated alias for --autonomy progressive)]"
                );
            }
            // Persist the autonomy level in the runtime store: nothing reads
            // the process environment for this key (AutonomyLevel::from_root
            // resolves via env_or_config), so the previous std::env::set_var
            // made the flag a silent no-op.
            let requested_autonomy =
                if let Some(level_str) = find_flag_value(&args[1..], "--autonomy") {
                    Some(autonomy::AutonomyLevel::from_str_lossy(level_str))
                } else if flags.contains(&"--auto-approve-gates") {
                    eprintln!(
                        "warning: --auto-approve-gates is deprecated; use --autonomy progressive"
                    );
                    Some(autonomy::AutonomyLevel::Progressive)
                } else {
                    None
                };
            if let Some(level) = requested_autonomy {
                let mut env_map = runtime_env::load_runtime_env_map(&root)?;
                env_map.insert(
                    "CTOX_AUTONOMY_LEVEL".to_string(),
                    level.as_str().to_string(),
                );
                runtime_env::save_runtime_env_map(&root, &env_map)?;
            }
            service::run_foreground(root)
        }
        Some("version") => {
            let version = version_info(root)?;
            println!("{}", serde_json::to_string_pretty(&version)?);
            Ok(())
        }
        Some("chat") => handle_chat(root, &args[1..]),
        Some("cost") => api_costs::handle_cost_command(root, &args[1..]),
        Some("start") => {
            println!("{}", service::start_background(root)?);
            Ok(())
        }
        Some("stop") => {
            let flags: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
            if flags.iter().any(|flag| *flag != "--force") {
                anyhow::bail!("usage: ctox stop [--force]");
            }
            println!(
                "{}",
                service::stop_background_guarded(root, flags.contains(&"--force"))?
            );
            Ok(())
        }
        Some("status") => {
            let probe = service::StatusProbeOptions {
                reconcile_runtime_switch: false,
                systemd_cache_ttl: Some(std::time::Duration::from_secs(5)),
                manager_probe: false,
                status_ipc_timeout: Some(std::time::Duration::from_secs(1)),
                lifecycle_alerts: false,
                include_business_os: false,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&service::service_status_snapshot_with(
                    root, &probe
                )?)?
            );
            Ok(())
        }
        Some("work-hours") => service::working_hours::handle_work_hours_command(root, &args[1..]),
        Some("tui") => tui::run_tui(root),
        Some("business-os") | Some("business") => {
            service::business_os::handle_business_os_command(root, &args[1..])
        }
        Some("coding-agent") | Some("coding-agents") => coding_agents::handle_cli(root, &args[1..]),
        Some("turn") => service::turn_ledger::handle_turn_command(root, &args[1..]),
        Some("harness-flow") => {
            service::harness_flow::handle_harness_flow_command(root, &args[1..])
        }
        Some("process-mining") => {
            service::process_mining::handle_process_mining_command(root, &args[1..])
        }
        Some("harness-mining") => {
            service::harness_mining::handle_harness_mining_command(root, &args[1..])
        }
        Some("reset") => service::reset::handle_reset_command(root, &args[1..]),
        Some("tui-smoke") => {
            let page = args.get(1).map(String::as_str).unwrap_or("chat");
            let width: u16 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(120);
            let height: u16 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(40);
            tui::run_tui_smoke(&root, page, width, height)
        }
        Some("browser") => browser::handle_browser_command(&root, &args[1..]),
        Some("appsec") => handle_appsec_command(&root, &args[1..]),
        Some("channel") => channels::handle_channel_command(&root, &args[1..]),
        Some("mailserver") => handle_mailserver_command(&root, &args[1..]),
        Some("doc") => doc::handle_doc_command(&root, &args[1..]),
        Some("follow-up") => follow_up::handle_follow_up_command(&args[1..]),
        Some("governance") => governance::handle_governance_command(&root, &args[1..]),
        Some("jami-daemon") => communication::jami_native::handle_daemon_command(&root, &args[1..]),
        Some("knowledge") => service::run_knowledge_data(&root, &args[1..]),
        Some("meeting") => communication::meeting_native::handle_meeting_command(&root, &args[1..]),
        Some("iot") => iot::commands::handle_iot_command(&root, &args[1..]),
        Some("plan") => plan::handle_plan_command(&root, &args[1..]),
        Some("queue") => queue::handle_queue_command(&root, &args[1..]),
        Some("report") => report::cli::handle_command(&root, &args[1..]),
        Some("scrape") => scrape::handle_scrape_command(&root, &args[1..]),
        Some("secret") => secrets::handle_secret_command(&root, &args[1..]),
        Some("skills") => handle_skills_command(&root, &args[1..]),
        Some("schedule") => schedule::handle_schedule_command(&root, &args[1..]),
        Some("strategy") => strategy::handle_strategy_command(&root, &args[1..]),
        Some("ticket") => tickets::handle_ticket_command(&root, &args[1..]),
        Some("web") => web::handle_web_command(&root, &args[1..]),
        Some("verification") => verification::handle_verification_command(&root, &args[1..]),
        Some("state-invariants") => {
            state_invariants::handle_state_invariants_command(&root, &args[1..])
        }
        Some("update") | Some("upgrade") => install::handle_update_command(&root, &args[1..]),
        Some("doctor") => install::handle_doctor_command(&root),
        Some("lcm-init") => {
            let db_path = args.get(1).context("usage: ctox lcm-init <db-path>")?;
            lcm::run_init(PathBuf::from(db_path).as_path())
        }
        Some("lcm-add-message") => {
            let db_path = args.get(1).context(
                "usage: ctox lcm-add-message <db-path> <conversation-id> <role> <content>",
            )?;
            let conversation_id: i64 = args
                .get(2)
                .context(
                    "usage: ctox lcm-add-message <db-path> <conversation-id> <role> <content>",
                )?
                .parse()
                .context("failed to parse conversation id")?;
            let role = args.get(3).context(
                "usage: ctox lcm-add-message <db-path> <conversation-id> <role> <content>",
            )?;
            let content = args
                .get(4..)
                .filter(|parts| !parts.is_empty())
                .map(|parts| parts.join(" "))
                .context(
                    "usage: ctox lcm-add-message <db-path> <conversation-id> <role> <content>",
                )?;
            let message = lcm::run_add_message(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                role,
                &content,
            )?;
            println!("{}", serde_json::to_string_pretty(&message)?);
            Ok(())
        }
        Some("lcm-compact") => {
            let db_path = args.get(1).context(
                "usage: ctox lcm-compact <db-path> <conversation-id> [token-budget] [--force]",
            )?;
            let conversation_id: i64 = args
                .get(2)
                .context(
                    "usage: ctox lcm-compact <db-path> <conversation-id> [token-budget] [--force]",
                )?
                .parse()
                .context("failed to parse conversation id")?;
            let token_budget = args
                .get(3)
                .filter(|value| !value.starts_with("--"))
                .map(|value| value.parse())
                .transpose()
                .context("failed to parse token budget")?
                .unwrap_or(24_000_i64);
            let force = args.iter().any(|arg| arg == "--force");
            let result = lcm::run_compact(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                token_budget,
                force,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-grep") => {
            let db_path = args
                .get(1)
                .context("usage: ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]")?;
            let conversation_arg = args
                .get(2)
                .context("usage: ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]")?;
            let conversation_id = if conversation_arg == "all" {
                None
            } else {
                Some(
                    conversation_arg
                        .parse()
                        .context("failed to parse conversation id")?,
                )
            };
            let scope = args
                .get(3)
                .context("usage: ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]")?;
            let mode = args
                .get(4)
                .context("usage: ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]")?;
            let tail = args
                .get(5..)
                .filter(|parts| !parts.is_empty())
                .context("usage: ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]")?;
            let (query, limit) = if let Some(last) = tail.last() {
                if let Ok(limit) = last.parse::<usize>() {
                    (tail[..tail.len().saturating_sub(1)].join(" "), limit)
                } else {
                    (tail.join(" "), 20_usize)
                }
            } else {
                anyhow::bail!(
                    "usage: ctox lcm-grep <db-path> <conversation-id|all> <scope> <mode> <query> [limit]"
                );
            };
            let result = lcm::run_grep(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                scope,
                mode,
                &query,
                limit,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-describe") => {
            let db_path = args
                .get(1)
                .context("usage: ctox lcm-describe <db-path> <summary-id>")?;
            let id = args
                .get(2)
                .context("usage: ctox lcm-describe <db-path> <summary-id>")?;
            let result = lcm::run_describe(PathBuf::from(db_path).as_path(), id)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-expand") => {
            let db_path = args.get(1).context(
                "usage: ctox lcm-expand <db-path> <summary-id> [depth] [--messages] [token-cap]",
            )?;
            let summary_id = args.get(2).context(
                "usage: ctox lcm-expand <db-path> <summary-id> [depth] [--messages] [token-cap]",
            )?;
            let numeric_args = args
                .iter()
                .skip(3)
                .filter(|value| !value.starts_with("--"))
                .collect::<Vec<_>>();
            let depth = numeric_args
                .first()
                .map(|value| value.parse())
                .transpose()
                .context("failed to parse depth")?
                .unwrap_or(1_usize);
            let include_messages = args.iter().any(|arg| arg == "--messages");
            let token_cap = numeric_args
                .get(1)
                .map(|value| value.parse())
                .transpose()
                .context("failed to parse token cap")?
                .unwrap_or(8_000_i64);
            let result = lcm::run_expand(
                PathBuf::from(db_path).as_path(),
                summary_id,
                depth,
                include_messages,
                token_cap,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-dump") => {
            let db_path = args
                .get(1)
                .context("usage: ctox lcm-dump <db-path> <conversation-id>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox lcm-dump <db-path> <conversation-id>")?
                .parse()
                .context("failed to parse conversation id")?;
            let result = lcm::run_dump(PathBuf::from(db_path).as_path(), conversation_id)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-refresh-continuity") => {
            let db_path = args
                .get(1)
                .context("usage: ctox lcm-refresh-continuity <db-path> <conversation-id>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox lcm-refresh-continuity <db-path> <conversation-id>")?
                .parse()
                .context("failed to parse conversation id")?;
            let result =
                lcm::run_refresh_continuity(PathBuf::from(db_path).as_path(), conversation_id)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-show-continuity") => {
            let db_path = args
                .get(1)
                .context("usage: ctox lcm-show-continuity <db-path> <conversation-id>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox lcm-show-continuity <db-path> <conversation-id>")?
                .parse()
                .context("failed to parse conversation id")?;
            let result =
                lcm::run_show_continuity(PathBuf::from(db_path).as_path(), conversation_id)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("lcm-run-fixture") => {
            let db_path = args
                .get(1)
                .context("usage: ctox lcm-run-fixture <db-path> <fixture-path>")?;
            let fixture_path = args
                .get(2)
                .context("usage: ctox lcm-run-fixture <db-path> <fixture-path>")?;
            let result = lcm::run_fixture(
                PathBuf::from(db_path).as_path(),
                PathBuf::from(fixture_path).as_path(),
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-init") => {
            let db_path = args
                .get(1)
                .context("usage: ctox continuity-init <db-path> <conversation-id>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-init <db-path> <conversation-id>")?
                .parse()
                .context("failed to parse conversation id")?;
            let result =
                lcm::run_continuity_init(PathBuf::from(db_path).as_path(), conversation_id)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-show") => {
            let db_path = args.get(1).context(
                "usage: ctox continuity-show <db-path> <conversation-id> [narrative|anchors|focus]",
            )?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-show <db-path> <conversation-id> [narrative|anchors|focus]")?
                .parse()
                .context("failed to parse conversation id")?;
            let kind = args.get(3).map(String::as_str);
            let result =
                lcm::run_continuity_show(PathBuf::from(db_path).as_path(), conversation_id, kind)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-apply") => {
            let db_path = args
                .get(1)
                .context("usage: ctox continuity-apply <db-path> <conversation-id> <narrative|anchors|focus> <diff-path>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-apply <db-path> <conversation-id> <narrative|anchors|focus> <diff-path>")?
                .parse()
                .context("failed to parse conversation id")?;
            let kind = args
                .get(3)
                .context("usage: ctox continuity-apply <db-path> <conversation-id> <narrative|anchors|focus> <diff-path>")?;
            let diff_path = args
                .get(4)
                .context("usage: ctox continuity-apply <db-path> <conversation-id> <narrative|anchors|focus> <diff-path>")?;
            let result = lcm::run_continuity_apply(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                kind,
                PathBuf::from(diff_path).as_path(),
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-update") => handle_continuity_update(&args[1..]),
        Some("continuity-log") => {
            let db_path = args.get(1).context(
                "usage: ctox continuity-log <db-path> <conversation-id> [narrative|anchors|focus]",
            )?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-log <db-path> <conversation-id> [narrative|anchors|focus]")?
                .parse()
                .context("failed to parse conversation id")?;
            let kind = args.get(3).map(String::as_str);
            let result =
                lcm::run_continuity_log(PathBuf::from(db_path).as_path(), conversation_id, kind)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-rebuild") => {
            let db_path = args
                .get(1)
                .context("usage: ctox continuity-rebuild <db-path> <conversation-id> <narrative|anchors|focus>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-rebuild <db-path> <conversation-id> <narrative|anchors|focus>")?
                .parse()
                .context("failed to parse conversation id")?;
            let kind = args
                .get(3)
                .context("usage: ctox continuity-rebuild <db-path> <conversation-id> <narrative|anchors|focus>")?;
            let result = lcm::run_continuity_rebuild(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                kind,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-forgotten") => {
            let db_path = args
                .get(1)
                .context("usage: ctox continuity-forgotten <db-path> <conversation-id> [narrative|anchors|focus] [query]")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-forgotten <db-path> <conversation-id> [narrative|anchors|focus] [query]")?
                .parse()
                .context("failed to parse conversation id")?;
            let kind = args.get(3).map(String::as_str);
            let query = args
                .get(4..)
                .filter(|parts| !parts.is_empty())
                .map(|parts| parts.join(" "));
            let result = lcm::run_continuity_forgotten(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                kind,
                query.as_deref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("continuity-build-prompt") => {
            let db_path = args
                .get(1)
                .context("usage: ctox continuity-build-prompt <db-path> <conversation-id> <narrative|anchors|focus>")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox continuity-build-prompt <db-path> <conversation-id> <narrative|anchors|focus>")?
                .parse()
                .context("failed to parse conversation id")?;
            let kind = args
                .get(3)
                .context("usage: ctox continuity-build-prompt <db-path> <conversation-id> <narrative|anchors|focus>")?;
            let result = lcm::run_continuity_build_prompt(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                kind,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("context-health") => {
            let db_path = args
                .get(1)
                .context("usage: ctox context-health <db-path> <conversation-id> [latest-user-prompt] [token-budget]")?;
            let conversation_id: i64 = args
                .get(2)
                .context("usage: ctox context-health <db-path> <conversation-id> [latest-user-prompt] [token-budget]")?
                .parse()
                .context("failed to parse conversation id")?;
            let tail = args.get(3..).unwrap_or(&[]);
            let (latest_prompt, token_budget) = if let Some(last) = tail.last() {
                if let Ok(token_budget) = last.parse::<i64>() {
                    (
                        (!tail[..tail.len().saturating_sub(1)].is_empty())
                            .then(|| tail[..tail.len().saturating_sub(1)].join(" ")),
                        token_budget,
                    )
                } else {
                    (Some(tail.join(" ")), 262_144_i64)
                }
            } else {
                (None, 262_144_i64)
            };
            let result = context_health::assess_for_conversation(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                token_budget,
                latest_prompt.as_deref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("chat-prompt-export") => {
            let conversation_id: i64 = find_flag_value(&args[1..], "--conversation-id")
                .unwrap_or("1")
                .parse()
                .context("failed to parse conversation id")?;
            let db_path = find_flag_value(&args[1..], "--db")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("runtime/ctox.sqlite3"));
            let output_path = find_flag_value(&args[1..], "--output").map(PathBuf::from);
            let settings = runtime_env::effective_runtime_env_map(&root).unwrap_or_default();
            let model = runtime_env::effective_chat_model_from_map(&settings)
                .unwrap_or_else(|| model_registry::default_local_chat_model().to_string());
            let artifact = live_context::render_live_prompt_artifact(
                &root,
                &settings,
                &model,
                db_path.as_path(),
                conversation_id,
            )?;
            let markdown = artifact.to_review_markdown();
            if let Some(path) = output_path {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create parent directory for {}", path.display())
                    })?;
                }
                std::fs::write(&path, markdown)
                    .with_context(|| format!("failed to write {}", path.display()))?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "conversation_id": artifact.conversation_id,
                        "model": artifact.model,
                        "output_path": path,
                        "breakdown": artifact.breakdown,
                    }))?
                );
            } else {
                println!("{markdown}");
            }
            Ok(())
        }
        Some("context-stress") => {
            let db_path = args
                .get(1)
                .context("usage: ctox context-stress <db-path> [conversation-id] [iterations] [token-budget]")?;
            let conversation_id = args
                .get(2)
                .map(|value| value.parse::<i64>())
                .transpose()
                .context("failed to parse conversation id")?;
            let iterations = args
                .get(3)
                .map(|value| value.parse::<usize>())
                .transpose()
                .context("failed to parse iterations")?;
            let token_budget = args
                .get(4)
                .map(|value| value.parse::<i64>())
                .transpose()
                .context("failed to parse token budget")?;
            let result = context_stress::run_context_stress(
                PathBuf::from(db_path).as_path(),
                conversation_id,
                iterations,
                token_budget,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some("context-retrieve") => {
            let conversation_id: i64 = find_flag_value(&args[1..], "--conversation-id")
                .unwrap_or("1")
                .parse()
                .context("failed to parse conversation id")?;
            let mode = find_flag_value(&args[1..], "--mode").unwrap_or("current");
            let db_path = find_flag_value(&args[1..], "--db")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("runtime/ctox.sqlite3"));
            let query = find_flag_value(&args[1..], "--query").map(ToOwned::to_owned);
            let continuity_kind = find_flag_value(&args[1..], "--kind").map(ToOwned::to_owned);
            let summary_id = find_flag_value(&args[1..], "--summary-id").map(ToOwned::to_owned);
            let limit = find_flag_value(&args[1..], "--limit")
                .map(|value| value.parse::<usize>())
                .transpose()
                .context("failed to parse limit")?
                .unwrap_or(10);
            let depth = find_flag_value(&args[1..], "--depth")
                .map(|value| value.parse::<usize>())
                .transpose()
                .context("failed to parse depth")?
                .unwrap_or(1);
            let token_cap = find_flag_value(&args[1..], "--token-cap")
                .map(|value| value.parse::<i64>())
                .transpose()
                .context("failed to parse token cap")?
                .unwrap_or(8_000);
            let include_messages = args.iter().any(|arg| arg == "--messages");
            let result = lcm::run_context_retrieve(
                db_path.as_path(),
                conversation_id,
                mode,
                query.as_deref(),
                continuity_kind.as_deref(),
                summary_id.as_deref(),
                limit,
                depth,
                include_messages,
                token_cap,
            )?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        Some(unknown) => {
            anyhow::bail!("unknown command `{unknown}` — run `ctox help` for usage");
        }
    }
}

fn handle_appsec_command(root: &Path, args: &[String]) -> anyhow::Result<()> {
    if matches!(
        appsec_command_pair(args).map(|(command, _)| command),
        Some("state" | "durable")
    ) {
        let output = appsec_state::handle_state_command(root, args)
            .context("ctox appsec state command failed")?;
        println!("{}", serde_json::to_string_pretty(&output)?);
        let ok = output.get("ok").and_then(serde_json::Value::as_bool) != Some(false);
        if ok {
            return Ok(());
        }
        anyhow::bail!("ctox appsec state command failed");
    }

    if is_appsec_pipeline_work(args) {
        let output = handle_appsec_pipeline_work(root, args)
            .context("ctox appsec pipeline work command failed")?;
        let ok = output.get("ok").and_then(serde_json::Value::as_bool) != Some(false);
        println!("{}", serde_json::to_string_pretty(&output)?);
        if ok {
            return Ok(());
        }
        anyhow::bail!("ctox appsec pipeline work command failed");
    }

    let output = run_projected_appsec_command(root, args)?;
    let ok = output.get("ok").and_then(serde_json::Value::as_bool) != Some(false);
    println!("{}", serde_json::to_string_pretty(&output)?);
    if ok {
        Ok(())
    } else {
        anyhow::bail!("ctox appsec command failed")
    }
}

fn build_appsec_forwarded_args(root: &Path, args: &[String]) -> Vec<String> {
    let mut forwarded = Vec::with_capacity(args.len() + 5);
    forwarded.push("ctox-appsec".to_string());
    let has_state_dir = args.iter().any(|arg| arg == "--state-dir")
        || std::env::var("PENTEST_STATE_DIR").is_ok_and(|value| !value.trim().is_empty());
    if !has_state_dir {
        forwarded.push("--state-dir".to_string());
        forwarded.push(
            root.join("runtime/appsec/default")
                .to_string_lossy()
                .to_string(),
        );
    }
    let has_tools_root = args.iter().any(|arg| arg == "--tools-root");
    if !has_tools_root {
        forwarded.push("--tools-root".to_string());
        forwarded.push(
            root.join("runtime/tools/appsec")
                .to_string_lossy()
                .to_string(),
        );
    }
    forwarded.extend(args.iter().cloned());
    forwarded
}

fn appsec_state_dir_for_args(root: &Path, args: &[String]) -> PathBuf {
    if let Some(state_dir) = arg_value(args, "--state-dir") {
        return PathBuf::from(state_dir);
    }
    if let Ok(state_dir) = std::env::var("PENTEST_STATE_DIR") {
        if !state_dir.trim().is_empty() {
            return PathBuf::from(state_dir);
        }
    }
    root.join("runtime/appsec/default")
}

pub(crate) fn run_projected_appsec_command(root: &Path, args: &[String]) -> anyhow::Result<Value> {
    let args = append_appsec_credential_proof_arg(root, args)?;
    let forwarded = build_appsec_forwarded_args(root, &args);
    let mut output = ctox_appsec_pentest::run_cli_json(forwarded.clone(), Some(root.to_path_buf()))
        .context("ctox appsec command failed")?;
    let ok = output.get("ok").and_then(serde_json::Value::as_bool) != Some(false);
    if ok && is_appsec_pipeline_enqueue(&args) {
        let enqueue = enqueue_appsec_pipeline_queue_tasks(root, &output)
            .context("failed to enqueue AppSec pipeline stages")?;
        if let Some(object) = output.as_object_mut() {
            object.insert("ctox_queue_enqueue".to_string(), enqueue);
        }
    }
    let projection = appsec_state::project_cli_result(root, &forwarded, &output)
        .context("failed to project ctox appsec result into durable state")?;
    if let Some(object) = output.as_object_mut() {
        object.insert("ctox_durable_projection".to_string(), projection);
    }
    Ok(output)
}

fn append_appsec_credential_proof_arg(root: &Path, args: &[String]) -> anyhow::Result<Vec<String>> {
    let command_pair = appsec_command_pair(args);
    let should_prove = matches!(command_pair, Some(("authz", Some("run"))))
        || (matches!(command_pair, Some(("authz", Some("preflight"))))
            && arg_flag(args, "--require-credentials"));
    if !should_prove
        || arg_value(args, "--credential-proof").is_some()
        || arg_value(args, "--subjects").is_none()
    {
        return Ok(args.to_vec());
    }

    let subjects_path = resolve_appsec_cli_path(root, &arg_value(args, "--subjects").unwrap());
    let subjects_value: Value =
        serde_json::from_slice(&fs::read(&subjects_path).with_context(|| {
            format!(
                "failed to read AppSec authz subjects for credential proof: {}",
                subjects_path.display()
            )
        })?)?;
    let proof = build_appsec_credential_proof(root, &subjects_value)?;
    let state_dir = resolve_appsec_cli_path(root, &appsec_state_dir_for_args(root, args));
    let authz_dir = state_dir.join("authz");
    fs::create_dir_all(&authz_dir).with_context(|| {
        format!(
            "failed to create AppSec authz proof directory {}",
            authz_dir.display()
        )
    })?;
    let proof_path = authz_dir.join(format!("authz-credential-proof-{}.json", current_millis()));
    fs::write(&proof_path, serde_json::to_vec_pretty(&proof)?).with_context(|| {
        format!(
            "failed to write AppSec authz credential proof {}",
            proof_path.display()
        )
    })?;

    let mut augmented = args.to_vec();
    augmented.extend([
        "--credential-proof".to_string(),
        proof_path.to_string_lossy().to_string(),
    ]);
    Ok(augmented)
}

fn resolve_appsec_cli_path(root: &Path, value: impl AsRef<Path>) -> PathBuf {
    let path = value.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn build_appsec_credential_proof(root: &Path, subjects_value: &Value) -> anyhow::Result<Value> {
    let subjects = if let Some(items) = subjects_value.get("subjects").and_then(Value::as_array) {
        items.as_slice()
    } else if let Some(items) = subjects_value.as_array() {
        items.as_slice()
    } else {
        anyhow::bail!("authz subjects file must be an array or object with subjects[]");
    };
    let mut rows = Vec::new();
    let mut available = 0usize;
    let mut missing = 0usize;
    let mut empty = 0usize;
    let mut invalid = 0usize;
    let mut credential_subjects = 0usize;

    for subject in subjects {
        let subject_id = subject
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let role = subject
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .trim();
        let anonymous = role == "anonymous" || subject_id == "unauthenticated";
        let credential_ref = subject
            .get("credential_ref")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let (status, detail) = if anonymous {
            ("anonymous", "credential-not-required")
        } else if let Some(credential_ref) = credential_ref {
            credential_subjects += 1;
            match parse_appsec_ctox_secret_ref(credential_ref) {
                Some((scope, name)) => match crate::secrets::read_secret_value(root, &scope, &name)
                {
                    Ok(value) if !value.trim().is_empty() => {
                        available += 1;
                        ("available", "secret-present")
                    }
                    Ok(_) => {
                        empty += 1;
                        ("empty", "secret-empty")
                    }
                    Err(_) => {
                        if crate::secrets::secret_exists(root, &scope, &name).unwrap_or(false) {
                            missing += 1;
                            ("unreadable", "secret-present-but-unreadable")
                        } else {
                            missing += 1;
                            ("missing", "secret-missing")
                        }
                    }
                },
                None => {
                    invalid += 1;
                    ("invalid-ref", "credential-ref-invalid")
                }
            }
        } else {
            missing += 1;
            ("missing-ref", "credential-ref-missing")
        };
        rows.push(json!({
            "subject_id": subject_id,
            "role": role,
            "credential_ref": credential_ref,
            "status": status,
            "detail": detail,
        }));
    }

    Ok(json!({
        "version": "ctox.appsec_pentest.authz_credential_proof.v1",
        "generated_by": "ctox-core-secret-store",
        "generated_at": current_millis().to_string(),
        "subjects": rows,
        "summary": {
            "subjects": subjects.len(),
            "credential_subjects": credential_subjects,
            "available": available,
            "missing": missing,
            "empty": empty,
            "invalid": invalid,
        },
        "secret_policy": "This proof stores only redacted credential availability status from the CTOX Secret Store. It never stores passwords, cookies, bearer tokens, private keys, screenshots, raw browser streams, or decrypted secret values."
    }))
}

fn parse_appsec_ctox_secret_ref(value: &str) -> Option<(String, String)> {
    if value.chars().any(char::is_whitespace) {
        return None;
    }
    let Ok(parsed) = Url::parse(value) else {
        return None;
    };
    if parsed.scheme() != "ctox-secret"
        || parsed.username() != ""
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    let scope = parsed.host_str()?.trim();
    if scope.is_empty() || scope.contains('/') {
        return None;
    }
    let segments = parsed.path_segments()?.collect::<Vec<_>>();
    if segments.len() != 1 || segments[0].trim().is_empty() || segments[0].contains('/') {
        return None;
    }
    Some((scope.to_string(), segments[0].trim().to_string()))
}

fn is_appsec_pipeline_enqueue(args: &[String]) -> bool {
    matches!(
        appsec_command_pair(args),
        Some(("pipeline", Some("enqueue")))
    )
}

fn is_appsec_pipeline_work(args: &[String]) -> bool {
    matches!(
        appsec_command_pair(args),
        Some(("pipeline", Some("work" | "worker" | "run-queue")))
    )
}

fn appsec_command_pair(args: &[String]) -> Option<(&str, Option<&str>)> {
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--state-dir" | "--tools-root" => {
                index += 2;
            }
            "--json" => {
                index += 1;
            }
            value if value.starts_with('-') => {
                index += 1;
            }
            command => {
                return Some((command, args.get(index + 1).map(String::as_str)));
            }
        }
    }
    None
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window.first().map(String::as_str) == Some(flag))
        .and_then(|window| window.get(1))
        .cloned()
}

fn arg_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

pub(crate) fn handle_appsec_pipeline_work(root: &Path, args: &[String]) -> anyhow::Result<Value> {
    let state_dir = appsec_state_dir_for_args(root, args);
    let limit = arg_value(args, "--limit")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1);
    let dry_run = arg_flag(args, "--dry-run");
    let lease_owner = arg_value(args, "--lease-owner")
        .unwrap_or_else(|| "ctox-appsec-pipeline-worker".to_string());
    let message_key = arg_value(args, "--message-key");
    let candidates =
        select_appsec_pipeline_queue_tasks(root, &state_dir, message_key.as_deref(), limit)?;
    let mut results = Vec::new();
    for task in candidates.into_iter().take(limit) {
        let result = if dry_run {
            appsec_pipeline_worker_dry_run_task(root, &state_dir, &task)?
        } else {
            appsec_pipeline_worker_execute_task(root, &state_dir, &task, &lease_owner)?
        };
        results.push(result);
    }
    let succeeded = results
        .iter()
        .filter(|item| item.get("status").and_then(Value::as_str) == Some("handled"))
        .count();
    let blocked = results
        .iter()
        .filter(|item| item.get("status").and_then(Value::as_str) == Some("blocked"))
        .count();
    let retry_scheduled = results
        .iter()
        .filter(|item| item.get("status").and_then(Value::as_str) == Some("retry-scheduled"))
        .count();
    let failed = results
        .iter()
        .filter(|item| item.get("status").and_then(Value::as_str) == Some("failed"))
        .count();
    let output = json!({
        "ok": failed == 0,
        "command": "pipeline work",
        "version": "ctox.appsec.pipeline_worker.v1",
        "state_dir": state_dir.to_string_lossy(),
        "dry_run": dry_run,
        "lease_owner": lease_owner,
        "summary": {
            "selected": results.len(),
            "handled": succeeded,
            "blocked": blocked,
            "retry_scheduled": retry_scheduled,
            "failed": failed,
        },
        "tasks": results,
    });
    Ok(output)
}

fn select_appsec_pipeline_queue_tasks(
    root: &Path,
    state_dir: &Path,
    message_key: Option<&str>,
    limit: usize,
) -> anyhow::Result<Vec<channels::QueueTaskView>> {
    if let Some(message_key) = message_key {
        let Some(task) = channels::load_queue_task(root, message_key)? else {
            anyhow::bail!("queue task `{message_key}` was not found");
        };
        ensure_appsec_pipeline_task(root, state_dir, &task)?;
        if appsec_pipeline_retry_due(root, &task.message_key)? {
            return Ok(vec![task]);
        }
        return Ok(Vec::new());
    }

    let pending = vec!["pending".to_string()];
    let mut selected = Vec::new();
    for task in channels::list_queue_tasks(root, &pending, limit.saturating_mul(20).max(50))? {
        if is_appsec_pipeline_task_for_state(root, state_dir, &task)?
            && appsec_pipeline_retry_due(root, &task.message_key)?
        {
            selected.push(task);
            if selected.len() >= limit {
                break;
            }
        }
    }
    Ok(selected)
}

fn ensure_appsec_pipeline_task(
    root: &Path,
    state_dir: &Path,
    task: &channels::QueueTaskView,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        is_appsec_pipeline_task_for_state(root, state_dir, task)?,
        "queue task `{}` is not an AppSec pipeline task for {}",
        task.message_key,
        state_dir.display()
    );
    Ok(())
}

fn is_appsec_pipeline_task_for_state(
    root: &Path,
    state_dir: &Path,
    task: &channels::QueueTaskView,
) -> anyhow::Result<bool> {
    if task.suggested_skill.as_deref() != Some("appsec-pentest") {
        return Ok(false);
    }
    let Some(appsec_state_dir) =
        channels::queue_task_metadata_value(root, &task.message_key, "appsec_state_dir")?
    else {
        return Ok(false);
    };
    let Some(appsec_state_dir) = appsec_state_dir.as_str() else {
        return Ok(false);
    };
    Ok(path_string_matches(state_dir, appsec_state_dir))
}

fn path_string_matches(path: &Path, raw: &str) -> bool {
    if raw == path.to_string_lossy() {
        return true;
    }
    Path::new(raw) == path
}

fn appsec_pipeline_task_stage(
    root: &Path,
    task: &channels::QueueTaskView,
) -> anyhow::Result<Value> {
    channels::queue_task_metadata_value(root, &task.message_key, "stage")?
        .context("AppSec pipeline queue task is missing stage metadata")
}

fn appsec_pipeline_worker_dry_run_task(
    root: &Path,
    _state_dir: &Path,
    task: &channels::QueueTaskView,
) -> anyhow::Result<Value> {
    let stage = appsec_pipeline_task_stage(root, task)?;
    Ok(json!({
        "message_key": task.message_key,
        "status": "dry-run",
        "route_status": task.route_status,
        "stage": appsec_stage_summary(&stage),
        "commands": appsec_stage_command_plan(&stage),
    }))
}

fn appsec_pipeline_worker_execute_task(
    root: &Path,
    state_dir: &Path,
    task: &channels::QueueTaskView,
    lease_owner: &str,
) -> anyhow::Result<Value> {
    ensure_appsec_pipeline_task(root, state_dir, task)?;
    let leased = channels::lease_queue_task(root, &task.message_key, lease_owner)?;
    let stage = appsec_pipeline_task_stage(root, &leased)?;
    let execution = execute_appsec_stage_commands(root, state_dir, &stage)?;
    let commands = execution
        .get("commands")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let command_failures = commands
        .iter()
        .filter(|command| command.get("ok").and_then(Value::as_bool) == Some(false))
        .count();
    let command_blocks = commands
        .iter()
        .filter(|command| {
            command
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| status.starts_with("blocked"))
        })
        .count();
    let completed_artifacts = completed_run_artifacts(&commands);
    let mut coverage_update = Value::Null;
    let mut analyze_output = Value::Null;
    let mut pipeline_status = Value::Null;
    let final_status: String;
    let final_note: String;
    let mut failure_policy = Value::Null;

    if command_failures == 0 && command_blocks == 0 && !completed_artifacts.is_empty() {
        analyze_output = run_projected_appsec_command(
            root,
            &[
                "analyze".to_string(),
                "--state-dir".to_string(),
                state_dir.to_string_lossy().to_string(),
            ],
        )?;
        coverage_update =
            mark_appsec_stage_coverage(root, state_dir, &stage, &completed_artifacts)?;
        pipeline_status = run_projected_appsec_command(
            root,
            &[
                "pipeline".to_string(),
                "status".to_string(),
                "--state-dir".to_string(),
                state_dir.to_string_lossy().to_string(),
            ],
        )?;
        if appsec_pipeline_stage_terminal(&pipeline_status, &stage) {
            let note = format!(
                "appsec:terminal-success: pipeline stage completed with coverage evidence for {}",
                appsec_stage_id(&stage).unwrap_or("unknown-stage")
            );
            channels::update_queue_task(
                root,
                channels::QueueTaskUpdateRequest {
                    message_key: leased.message_key.clone(),
                    route_status: Some("handled".to_string()),
                    status_note: Some(note.clone()),
                    ..Default::default()
                },
            )?;
            final_status = "handled".to_string();
            final_note = note;
        } else {
            channels::update_queue_task(
                root,
                channels::QueueTaskUpdateRequest {
                    message_key: leased.message_key.clone(),
                    route_status: Some("blocked".to_string()),
                    status_note: Some(
                        "AppSec commands ran, but pipeline status is not terminal for this stage"
                            .to_string(),
                    ),
                    ..Default::default()
                },
            )?;
            final_status = "blocked".to_string();
            final_note = "AppSec commands ran, but pipeline status is not terminal for this stage"
                .to_string();
        }
    } else {
        let note = appsec_stage_worker_blocker_note(
            command_failures,
            command_blocks,
            &completed_artifacts,
        );
        if command_failures > 0 && appsec_stage_failure_is_retryable(&commands) {
            failure_policy =
                apply_appsec_stage_failure_retry_policy(root, state_dir, &leased, &stage, &note)?;
            final_status = failure_policy
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("failed")
                .to_string();
            final_note = failure_policy
                .get("note")
                .and_then(Value::as_str)
                .unwrap_or(&note)
                .to_string();
        } else {
            channels::update_queue_task(
                root,
                channels::QueueTaskUpdateRequest {
                    message_key: leased.message_key.clone(),
                    route_status: Some("blocked".to_string()),
                    status_note: Some(note.clone()),
                    ..Default::default()
                },
            )?;
            failure_policy = json!({
                "version": APPSEC_PIPELINE_RETRY_VERSION,
                "status": "blocked",
                "retryable": false,
                "reason": "stage command failure is a durable blocker, not a transient tool failure",
                "max_attempts": APPSEC_PIPELINE_STAGE_MAX_ATTEMPTS,
                "command_statuses": appsec_stage_command_statuses(&commands),
            });
            final_status = "blocked".to_string();
            final_note = note;
        }
    }

    let refreshed = channels::load_queue_task(root, &leased.message_key)?.unwrap_or(leased);
    let result = json!({
        "message_key": refreshed.message_key,
        "status": final_status,
        "note": final_note,
        "route_status": refreshed.route_status,
        "stage": appsec_stage_summary(&stage),
        "execution": execution,
        "failure_policy": failure_policy,
        "analysis": analyze_output,
        "coverage_update": coverage_update,
        "pipeline_status": pipeline_status,
    });
    channels::set_queue_task_metadata_value(
        root,
        &refreshed.message_key,
        "appsec_worker_result",
        result.clone(),
    )?;
    run_projected_appsec_command(
        root,
        &[
            "pipeline".to_string(),
            "status".to_string(),
            "--state-dir".to_string(),
            state_dir.to_string_lossy().to_string(),
        ],
    )?;
    Ok(result)
}

fn appsec_pipeline_retry_due(root: &Path, message_key: &str) -> anyhow::Result<bool> {
    let Some(retry) = channels::queue_task_metadata_value(root, message_key, "appsec_retry")?
    else {
        return Ok(true);
    };
    let Some(not_before) = retry
        .get("not_before")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(true);
    };
    Ok(not_before <= appsec_pipeline_retry_now_iso().as_str())
}

fn appsec_pipeline_retry_now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn appsec_pipeline_retry_not_before_iso() -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(APPSEC_PIPELINE_STAGE_RETRY_DELAY_SECONDS))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

fn appsec_stage_failure_is_retryable(commands: &[Value]) -> bool {
    let failed_statuses = appsec_stage_command_statuses(commands);
    !failed_statuses.is_empty()
        && failed_statuses.iter().all(|status| {
            matches!(
                status.as_str(),
                "failed" | "failed-command-error" | "blocked-tool-failed" | "blocked-tool-timeout"
            )
        })
}

fn appsec_stage_command_statuses(commands: &[Value]) -> Vec<String> {
    commands
        .iter()
        .filter(|command| command.get("ok").and_then(Value::as_bool) == Some(false))
        .filter_map(|command| command.get("status").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect()
}

fn appsec_stage_retry_attempts(root: &Path, message_key: &str) -> anyhow::Result<u64> {
    Ok(
        channels::queue_task_metadata_value(root, message_key, "appsec_retry")?
            .and_then(|retry| {
                retry
                    .get("failed_attempts")
                    .and_then(Value::as_u64)
                    .or_else(|| retry.get("attempt").and_then(Value::as_u64))
            })
            .unwrap_or(0),
    )
}

fn apply_appsec_stage_failure_retry_policy(
    root: &Path,
    state_dir: &Path,
    leased: &channels::QueueTaskView,
    stage: &Value,
    note: &str,
) -> anyhow::Result<Value> {
    let failed_attempts = appsec_stage_retry_attempts(root, &leased.message_key)? + 1;
    let remaining_attempts = APPSEC_PIPELINE_STAGE_MAX_ATTEMPTS.saturating_sub(failed_attempts);
    let retryable = failed_attempts < APPSEC_PIPELINE_STAGE_MAX_ATTEMPTS;
    let now = appsec_pipeline_retry_now_iso();
    let not_before = if retryable {
        Value::String(appsec_pipeline_retry_not_before_iso())
    } else {
        Value::Null
    };
    let retry_metadata = json!({
        "version": APPSEC_PIPELINE_RETRY_VERSION,
        "state_dir": state_dir.to_string_lossy(),
        "stage_id": appsec_stage_id(stage).unwrap_or("unknown-stage"),
        "failed_attempts": failed_attempts,
        "max_attempts": APPSEC_PIPELINE_STAGE_MAX_ATTEMPTS,
        "remaining_attempts": remaining_attempts,
        "retryable": retryable,
        "failed_at": now,
        "not_before": not_before,
        "last_failure_note": note,
    });
    if retryable {
        let retry_note = format!(
            "AppSec stage failed transiently; retry {}/{} scheduled after {}",
            failed_attempts,
            APPSEC_PIPELINE_STAGE_MAX_ATTEMPTS,
            retry_metadata
                .get("not_before")
                .and_then(Value::as_str)
                .unwrap_or("the retry delay")
        );
        channels::update_queue_task(
            root,
            channels::QueueTaskUpdateRequest {
                message_key: leased.message_key.clone(),
                route_status: Some("pending".to_string()),
                status_note: Some(retry_note.clone()),
                ..Default::default()
            },
        )?;
        channels::set_queue_task_metadata_value(
            root,
            &leased.message_key,
            "appsec_retry",
            retry_metadata,
        )?;
        Ok(json!({
            "version": APPSEC_PIPELINE_RETRY_VERSION,
            "status": "retry-scheduled",
            "route_status": "pending",
            "retryable": true,
            "failed_attempts": failed_attempts,
            "max_attempts": APPSEC_PIPELINE_STAGE_MAX_ATTEMPTS,
            "remaining_attempts": remaining_attempts,
            "note": retry_note,
        }))
    } else {
        channels::ack_leased_messages_with_failure_reason(
            root,
            std::slice::from_ref(&leased.message_key),
            "failed",
            note,
        )?;
        channels::set_queue_task_metadata_value(
            root,
            &leased.message_key,
            "appsec_retry",
            retry_metadata,
        )?;
        Ok(json!({
            "version": APPSEC_PIPELINE_RETRY_VERSION,
            "status": "failed",
            "route_status": "failed",
            "retryable": false,
            "failed_attempts": failed_attempts,
            "max_attempts": APPSEC_PIPELINE_STAGE_MAX_ATTEMPTS,
            "remaining_attempts": 0,
            "note": note,
        }))
    }
}

fn execute_appsec_stage_commands(
    root: &Path,
    state_dir: &Path,
    stage: &Value,
) -> anyhow::Result<Value> {
    let mut execution_context = AppsecStageExecutionContext::default();
    let mut command_results = Vec::new();
    let commands = stage
        .get("run_commands")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for command in commands {
        let (command, resolved_placeholders) =
            match resolve_appsec_stage_command_placeholders(command, &execution_context) {
                Ok(resolved) => resolved,
                Err(err) => {
                    command_results.push(json!({
                        "ok": false,
                        "status": "blocked-invalid-command",
                        "error": err.to_string(),
                    }));
                    continue;
                }
            };
        let command_kind = command
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if command
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| status.starts_with("blocked"))
        {
            command_results.push(json!({
                "ok": false,
                "status": command.get("status").and_then(Value::as_str).unwrap_or("blocked"),
                "kind": command_kind,
                "tool": command.get("tool").cloned().unwrap_or(Value::Null),
                "requires": command.get("requires").cloned().unwrap_or(Value::Null),
                "error": command.get("error").cloned().unwrap_or(Value::Null),
            }));
            continue;
        }
        let argv = command
            .get("argv")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !appsec_pipeline_command_is_ctox_contract(command_kind)
            && command.get("ctox_cli").is_none()
        {
            command_results.push(json!({
                "ok": false,
                "status": "blocked-external-tooling-required",
                "kind": command_kind,
                "argv": argv,
                "error": "pipeline stage command requires non-CLI CTOX tooling or external evidence",
            }));
            continue;
        }
        let argv_strings = match appsec_command_argv_strings(&command) {
            Ok(argv_strings) => argv_strings,
            Err(err) => {
                command_results.push(json!({
                    "ok": false,
                    "status": "blocked-invalid-command",
                    "argv": argv,
                    "error": err.to_string(),
                }));
                continue;
            }
        };
        if argv_strings.len() < 3 || argv_strings.first().map(String::as_str) != Some("ctox") {
            command_results.push(json!({
                "ok": false,
                "status": "blocked-invalid-command",
                "argv": argv_strings,
                "error": "pipeline worker only executes allowed CTOX CLI contracts",
            }));
            continue;
        }
        let unresolved_placeholders =
            appsec_command_unresolved_placeholders(&command, &argv_strings);
        if !unresolved_placeholders.is_empty() {
            command_results.push(json!({
                "ok": false,
                "status": "blocked-placeholder-required",
                "argv": argv_strings,
                "unresolved_placeholders": unresolved_placeholders,
                "error": "pipeline command contains an unresolved placeholder",
            }));
            continue;
        }
        if argv_strings.get(1).map(String::as_str) == Some("appsec")
            && argv_strings.get(2).map(String::as_str) == Some("pipeline")
            && argv_strings.get(3).map(String::as_str) == Some("work")
        {
            command_results.push(json!({
                "ok": false,
                "status": "blocked-recursive-worker-command",
                "argv": argv_strings,
                "error": "pipeline worker refuses to invoke itself recursively",
            }));
            continue;
        }
        let execution_output = match appsec_reusable_expected_artifact_output(state_dir, &command)?
        {
            Some(reused) => Ok(reused),
            None => execute_appsec_ctox_cli_command(root, state_dir, &command, &argv_strings)
                .map(|output| (output, None)),
        };
        match execution_output {
            Ok(output) => {
                let (output, reused_artifact) = output;
                let persisted_artifact = if reused_artifact.is_some() {
                    reused_artifact
                } else {
                    persist_appsec_command_expected_artifact(state_dir, &command, &output)?
                };
                let session_bindings =
                    record_appsec_stage_session_bindings(&mut execution_context, &command, &output);
                let artifact_bindings = record_appsec_stage_artifact_bindings(
                    &mut execution_context,
                    &command,
                    &output,
                    persisted_artifact.as_deref(),
                );
                let ok = output.get("ok").and_then(Value::as_bool) != Some(false);
                let status = output
                    .get("run")
                    .and_then(|run| run.get("status"))
                    .and_then(Value::as_str)
                    .or_else(|| output.get("status").and_then(Value::as_str))
                    .unwrap_or(if ok { "completed" } else { "failed" });
                let mut command_result = json!({
                    "ok": ok,
                    "status": status,
                    "argv": argv_strings,
                    "output": output,
                });
                if let Some(artifact) = persisted_artifact {
                    if let Some(object) = command_result.as_object_mut() {
                        object.insert("artifact_path".to_string(), Value::String(artifact));
                    }
                }
                insert_non_empty_array(
                    &mut command_result,
                    "resolved_placeholders",
                    resolved_placeholders,
                );
                insert_non_empty_array(&mut command_result, "session_bindings", session_bindings);
                insert_non_empty_array(&mut command_result, "artifact_bindings", artifact_bindings);
                let stop_on_failure =
                    command.get("stop_on_failure").and_then(Value::as_bool) == Some(true);
                command_results.push(command_result);
                if stop_on_failure && !ok {
                    break;
                }
            }
            Err(err) => {
                command_results.push(json!({
                    "ok": false,
                    "status": "failed-command-error",
                    "argv": argv_strings,
                    "error": err.to_string(),
                }));
            }
        }
    }
    Ok(json!({
        "commands": command_results,
        "session_bindings": appsec_stage_session_bindings_value(&execution_context),
        "artifact_bindings": appsec_stage_artifact_bindings_value(&execution_context),
    }))
}

#[derive(Debug, Default)]
struct AppsecStageExecutionContext {
    session_ids: BTreeMap<String, String>,
    artifacts: BTreeMap<String, String>,
}

fn appsec_pipeline_command_is_ctox_contract(command_kind: &str) -> bool {
    matches!(command_kind, "ctox-cli" | "ctox-web-stack-authz")
}

fn appsec_command_argv_strings(command: &Value) -> anyhow::Result<Vec<String>> {
    if let Some(argv) = command.get("argv") {
        return value_string_array(argv).context("pipeline command argv must be a string array");
    }
    let ctox_cli = command
        .get("ctox_cli")
        .context("pipeline command is missing argv or ctox_cli")?;
    let program = ctox_cli
        .get("program")
        .and_then(Value::as_str)
        .context("ctox_cli.program is required")?;
    let mut argv = vec![program.to_string()];
    if let Some(args) = ctox_cli.get("args") {
        argv.extend(value_string_array(args).context("ctox_cli.args must be a string array")?);
    }
    Ok(argv)
}

fn value_string_array(value: &Value) -> anyhow::Result<Vec<String>> {
    let values = value.as_array().context("value is not an array")?;
    values
        .iter()
        .map(|item| {
            item.as_str()
                .map(ToString::to_string)
                .context("array contains a non-string value")
        })
        .collect()
}

fn resolve_appsec_stage_command_placeholders(
    mut command: Value,
    context: &AppsecStageExecutionContext,
) -> anyhow::Result<(Value, Vec<Value>)> {
    let mut resolved_placeholders = Vec::new();
    replace_appsec_session_placeholders_in_string_array(
        &mut command,
        "/argv",
        "argv",
        context,
        &mut resolved_placeholders,
    )?;
    replace_appsec_session_placeholders_in_string_array(
        &mut command,
        "/ctox_cli/args",
        "ctox_cli.args",
        context,
        &mut resolved_placeholders,
    )?;
    replace_appsec_session_placeholder_in_string(
        &mut command,
        "/harness_tool/freeform_source",
        "harness_tool.freeform_source",
        context,
        &mut resolved_placeholders,
    )?;
    Ok((command, resolved_placeholders))
}

fn replace_appsec_session_placeholders_in_string_array(
    command: &mut Value,
    pointer: &str,
    field: &str,
    context: &AppsecStageExecutionContext,
    resolved_placeholders: &mut Vec<Value>,
) -> anyhow::Result<()> {
    let Some(value) = command.pointer_mut(pointer) else {
        return Ok(());
    };
    let array = value
        .as_array_mut()
        .with_context(|| format!("{field} must be a string array"))?;
    for item in array {
        let Some(text) = item.as_str() else {
            anyhow::bail!("{field} contains a non-string value");
        };
        let replacement = replace_appsec_session_placeholders_in_text(
            text,
            field,
            context,
            resolved_placeholders,
        );
        if replacement != text {
            *item = Value::String(replacement);
        }
    }
    Ok(())
}

fn replace_appsec_session_placeholder_in_string(
    command: &mut Value,
    pointer: &str,
    field: &str,
    context: &AppsecStageExecutionContext,
    resolved_placeholders: &mut Vec<Value>,
) -> anyhow::Result<()> {
    let Some(value) = command.pointer_mut(pointer) else {
        return Ok(());
    };
    let text = value
        .as_str()
        .with_context(|| format!("{field} must be a string"))?;
    let replacement =
        replace_appsec_session_placeholders_in_text(text, field, context, resolved_placeholders);
    if replacement != text {
        *value = Value::String(replacement);
    }
    Ok(())
}

fn replace_appsec_session_placeholders_in_text(
    text: &str,
    field: &str,
    context: &AppsecStageExecutionContext,
    resolved_placeholders: &mut Vec<Value>,
) -> String {
    let mut replacement = text.to_string();
    for (key, session_id) in &context.session_ids {
        let placeholder = appsec_session_placeholder_token(key);
        if replacement.contains(&placeholder) {
            replacement = replacement.replace(&placeholder, session_id);
            resolved_placeholders.push(json!({
                "field": field,
                "placeholder": placeholder,
                "binding": key,
                "value_ref": "ctox_web_stack_session_id",
            }));
        }
    }
    for (key, artifact) in &context.artifacts {
        let placeholder = appsec_artifact_placeholder_token(key);
        if replacement.contains(&placeholder) {
            replacement = replacement.replace(&placeholder, artifact);
            resolved_placeholders.push(json!({
                "field": field,
                "placeholder": placeholder,
                "binding": key,
                "value_ref": "ctox_appsec_artifact",
            }));
        }
    }
    replacement
}

fn appsec_command_unresolved_placeholders(command: &Value, argv: &[String]) -> Vec<Value> {
    let mut placeholders = Vec::new();
    for arg in argv {
        if appsec_argv_has_placeholder(arg) {
            placeholders.push(json!({
                "field": "argv",
                "value": appsec_placeholder_display(arg),
            }));
        }
    }
    if let Some(source) = command
        .pointer("/harness_tool/freeform_source")
        .and_then(Value::as_str)
    {
        for key in appsec_session_placeholder_keys_from_text(source) {
            placeholders.push(json!({
                "field": "harness_tool.freeform_source",
                "placeholder": appsec_session_placeholder_token(&key),
            }));
        }
        for key in appsec_artifact_placeholder_keys_from_text(source) {
            placeholders.push(json!({
                "field": "harness_tool.freeform_source",
                "placeholder": appsec_artifact_placeholder_token(&key),
            }));
        }
        if source.contains("${session_id:")
            && !placeholders.iter().any(|item| {
                item.get("field").and_then(Value::as_str) == Some("harness_tool.freeform_source")
            })
        {
            placeholders.push(json!({
                "field": "harness_tool.freeform_source",
                "value": "${session_id:*}",
            }));
        }
        if source.contains("${artifact:")
            && !placeholders.iter().any(|item| {
                item.get("field").and_then(Value::as_str) == Some("harness_tool.freeform_source")
            })
        {
            placeholders.push(json!({
                "field": "harness_tool.freeform_source",
                "value": "${artifact:*}",
            }));
        }
    }
    placeholders
}

fn appsec_argv_has_placeholder(arg: &str) -> bool {
    let trimmed = arg.trim();
    (trimmed.starts_with('<') && trimmed.ends_with('>'))
        || trimmed.contains("${")
        || trimmed.contains("<redacted")
        || trimmed.contains("<approval")
}

fn appsec_placeholder_display(value: &str) -> String {
    let trimmed = value.trim();
    const MAX_LEN: usize = 160;
    if trimmed.len() <= MAX_LEN {
        return trimmed.to_string();
    }
    format!("{}...", &trimmed[..MAX_LEN])
}

fn appsec_session_placeholder_token(key: &str) -> String {
    format!("${{session_id:{key}}}")
}

fn appsec_artifact_placeholder_token(key: &str) -> String {
    format!("${{artifact:{key}}}")
}

fn appsec_session_placeholder_keys_from_text(text: &str) -> Vec<String> {
    appsec_placeholder_keys_from_text(text, "${session_id:")
}

fn appsec_artifact_placeholder_keys_from_text(text: &str) -> Vec<String> {
    appsec_placeholder_keys_from_text(text, "${artifact:")
}

fn appsec_placeholder_keys_from_text(text: &str, marker: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = text[offset..].find(marker) {
        let key_start = offset + relative_start + marker.len();
        let Some(relative_end) = text[key_start..].find('}') else {
            break;
        };
        let key = text[key_start..key_start + relative_end].trim();
        if !key.is_empty() && !keys.iter().any(|existing| existing == key) {
            keys.push(key.to_string());
        }
        offset = key_start + relative_end + 1;
    }
    keys
}

fn record_appsec_stage_session_bindings(
    context: &mut AppsecStageExecutionContext,
    command: &Value,
    output: &Value,
) -> Vec<Value> {
    let Some(session_id) = appsec_output_session_id(output) else {
        return Vec::new();
    };
    let mut keys = appsec_command_session_placeholder_keys(command);
    if keys.is_empty() {
        if let Some(subject_id) = command.get("subject_id").and_then(Value::as_str) {
            keys.push(appsec_sanitize_session_placeholder_key(subject_id));
        }
    }
    let mut bindings = Vec::new();
    for key in keys {
        if key.trim().is_empty() {
            continue;
        }
        context.session_ids.insert(key.clone(), session_id.clone());
        bindings.push(json!({
            "binding": key,
            "session_id": session_id,
            "source": "ctox_web_stack_session_id",
        }));
    }
    bindings
}

fn record_appsec_stage_artifact_bindings(
    context: &mut AppsecStageExecutionContext,
    command: &Value,
    output: &Value,
    persisted_artifact: Option<&str>,
) -> Vec<Value> {
    let mut keys = appsec_command_artifact_placeholder_keys(command);
    if keys.is_empty() {
        return Vec::new();
    }
    let output_artifact = appsec_output_artifact(output);
    let persisted_artifact = persisted_artifact
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let artifact = if command.pointer("/produces/artifact/placeholder").is_some() {
        output_artifact.or(persisted_artifact)
    } else {
        persisted_artifact.or(output_artifact)
    };
    let Some(artifact) = artifact else {
        return Vec::new();
    };
    let mut bindings = Vec::new();
    for key in keys.drain(..) {
        if key.trim().is_empty() {
            continue;
        }
        context.artifacts.insert(key.clone(), artifact.clone());
        bindings.push(json!({
            "binding": key,
            "artifact": artifact,
            "source": "ctox_appsec_artifact",
        }));
    }
    bindings
}

fn appsec_output_session_id(output: &Value) -> Option<String> {
    [
        "/session_id",
        "/browser_session/session_id",
        "/result/session_id",
        "/output/session_id",
    ]
    .into_iter()
    .find_map(|pointer| {
        output
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn appsec_output_artifact(output: &Value) -> Option<String> {
    [
        "/artifact",
        "/matrix_artifact",
        "/plan_artifact",
        "/run/artifact",
        "/run/combined_artifact",
        "/output/artifact",
        "/import_result/artifact",
    ]
    .into_iter()
    .find_map(|pointer| {
        output
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn appsec_command_session_placeholder_keys(command: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    for pointer in [
        "/produces/session_id/placeholder",
        "/session_id_ref/placeholder",
        "/input/session_id",
        "/harness_tool/arguments/session_id",
        "/harness_tool/freeform_source",
    ] {
        let Some(value) = command.pointer(pointer).and_then(Value::as_str) else {
            continue;
        };
        for key in appsec_session_placeholder_keys_from_text(value) {
            if !keys.iter().any(|existing| existing == &key) {
                keys.push(key);
            }
        }
    }
    keys
}

fn appsec_command_artifact_placeholder_keys(command: &Value) -> Vec<String> {
    let mut keys = Vec::new();
    for pointer in [
        "/produces/artifact/placeholder",
        "/artifact_ref/placeholder",
        "/input/artifact",
        "/harness_tool/freeform_source",
    ] {
        let Some(value) = command.pointer(pointer).and_then(Value::as_str) else {
            continue;
        };
        for key in appsec_artifact_placeholder_keys_from_text(value) {
            if !keys.iter().any(|existing| existing == &key) {
                keys.push(key);
            }
        }
    }
    keys
}

fn appsec_sanitize_session_placeholder_key(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn appsec_stage_session_bindings_value(context: &AppsecStageExecutionContext) -> Value {
    Value::Object(
        context
            .session_ids
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect(),
    )
}

fn appsec_stage_artifact_bindings_value(context: &AppsecStageExecutionContext) -> Value {
    Value::Object(
        context
            .artifacts
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect(),
    )
}

fn insert_non_empty_array(target: &mut Value, key: &str, values: Vec<Value>) {
    if values.is_empty() {
        return;
    }
    if let Some(object) = target.as_object_mut() {
        object.insert(key.to_string(), Value::Array(values));
    }
}

fn execute_appsec_ctox_cli_command(
    root: &Path,
    state_dir: &Path,
    command: &Value,
    argv: &[String],
) -> anyhow::Result<Value> {
    match argv.get(1).map(String::as_str) {
        Some("appsec") => {
            let mut appsec_args = argv[2..].to_vec();
            if !appsec_args.iter().any(|arg| arg == "--state-dir") {
                appsec_args.push("--state-dir".to_string());
                appsec_args.push(state_dir.to_string_lossy().to_string());
            }
            run_projected_appsec_command(root, &appsec_args)
        }
        Some("web") => execute_appsec_web_cli_command(root, state_dir, command, &argv[2..]),
        Some("business-os") => execute_appsec_business_os_cli_command(root, &argv[2..]),
        Some(other) => anyhow::bail!("unsupported CTOX CLI domain `{other}` for AppSec worker"),
        None => anyhow::bail!("missing CTOX CLI domain"),
    }
}

fn execute_appsec_web_cli_command(
    root: &Path,
    state_dir: &Path,
    command: &Value,
    args: &[String],
) -> anyhow::Result<Value> {
    match args.first().map(String::as_str) {
        Some("browser-prepare") => crate::web_stack::prepare_browser_environment(
            root,
            &crate::web_stack::BrowserPrepareOptions {
                dir: arg_value(args, "--dir").map(PathBuf::from),
                install_reference: arg_flag(args, "--install-reference"),
                install_browser: arg_flag(args, "--install-browser"),
                skip_npm_install: arg_flag(args, "--skip-npm-install"),
            },
        ),
        Some("browser-automation") => {
            let source = browser_automation_source_from_stage_command(state_dir, command)?;
            if let Some(session_id) = arg_value(args, "--session-id") {
                return crate::business_os::run_browser_session_automation(
                    root,
                    crate::business_os::BrowserSessionAutomationRequest {
                        session_id,
                        dir: arg_value(args, "--dir").map(PathBuf::from),
                        timeout_ms: parse_optional_u64_arg(args, "--timeout-ms")?,
                        source,
                    },
                );
            }
            crate::web_stack::run_browser_automation(
                root,
                &crate::web_stack::BrowserAutomationRequest {
                    dir: arg_value(args, "--dir").map(PathBuf::from),
                    timeout_ms: parse_optional_u64_arg(args, "--timeout-ms")?,
                    source,
                },
            )
        }
        Some("browser-capture") => {
            let url = arg_value(args, "--url")
                .or_else(|| args.get(1).cloned())
                .context("ctox web browser-capture requires --url <url>")?;
            crate::web_stack::capture_browser_transport(
                root,
                &crate::web_stack::BrowserCaptureRequest {
                    dir: arg_value(args, "--dir").map(PathBuf::from),
                    out_dir: arg_value(args, "--out-dir").map(PathBuf::from),
                    timeout_ms: parse_optional_u64_arg(args, "--timeout-ms")?,
                    url,
                },
            )
        }
        Some(other) => anyhow::bail!("unsupported ctox web command `{other}` for AppSec worker"),
        None => anyhow::bail!("missing ctox web command"),
    }
}

fn browser_automation_source_from_stage_command(
    state_dir: &Path,
    command: &Value,
) -> anyhow::Result<String> {
    let stdin_contract = command.pointer("/ctox_cli/stdin").and_then(Value::as_str);
    if stdin_contract != Some("harness_tool.freeform_source") {
        anyhow::bail!(
            "ctox web browser-automation must use ctox_cli.stdin=harness_tool.freeform_source"
        );
    }
    let source = command
        .pointer("/harness_tool/freeform_source")
        .and_then(Value::as_str)
        .map(str::to_string)
        .context("browser automation command is missing harness_tool.freeform_source")?;
    anyhow::ensure!(
        !source.trim().is_empty(),
        "browser automation freeform source is empty"
    );
    let source = match appsec_authz_replay_candidates_source_prefix(state_dir, command)? {
        Some(prefix) => format!("{prefix}\n{source}"),
        None => source,
    };
    Ok(source)
}

fn appsec_authz_replay_candidates_source_prefix(
    state_dir: &Path,
    command: &Value,
) -> anyhow::Result<Option<String>> {
    if command.get("phase").and_then(Value::as_str) != Some("cross-subject-replay") {
        return Ok(None);
    }
    let artifact_ref = command
        .pointer("/input/owner_api_map_artifact")
        .and_then(Value::as_str)
        .context("cross-subject replay command is missing input.owner_api_map_artifact")?;
    let artifact_path = appsec_expected_artifact_path(state_dir, artifact_ref);
    let evidence: Value = serde_json::from_slice(&fs::read(&artifact_path).with_context(|| {
        format!(
            "failed to read owner API map artifact {}",
            artifact_path.display()
        )
    })?)
    .with_context(|| {
        format!(
            "failed to parse owner API map artifact {}",
            artifact_path.display()
        )
    })?;
    let candidates = appsec_authz_replay_candidates_from_evidence(&evidence);
    anyhow::ensure!(
        !candidates.is_empty(),
        "owner API map artifact {} produced no replay candidates",
        artifact_path.display()
    );
    Ok(Some(format!(
        "globalThis.ctoxAuthzReplayCandidates = {};",
        serde_json::to_string(&candidates)?
    )))
}

fn appsec_authz_replay_candidates_from_evidence(evidence: &Value) -> Vec<Value> {
    let mut candidates = Vec::new();
    let mut seen = BTreeMap::<String, ()>::new();
    collect_appsec_authz_replay_candidates(evidence, &mut candidates, &mut seen);
    candidates
}

fn collect_appsec_authz_replay_candidates(
    value: &Value,
    candidates: &mut Vec<Value>,
    seen: &mut BTreeMap<String, ()>,
) {
    match value {
        Value::Object(object) => {
            if let Some(candidate) = appsec_authz_candidate_from_object(value) {
                let endpoint = candidate
                    .get("endpoint")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let method = candidate
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("GET");
                let key = format!("{method} {endpoint}");
                if !seen.contains_key(&key) {
                    seen.insert(key, ());
                    candidates.push(candidate);
                }
            }
            for child in object.values() {
                collect_appsec_authz_replay_candidates(child, candidates, seen);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_appsec_authz_replay_candidates(item, candidates, seen);
            }
        }
        _ => {}
    }
}

fn appsec_authz_candidate_from_object(value: &Value) -> Option<Value> {
    let endpoint = value
        .get("endpoint")
        .or_else(|| value.get("url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())?;
    if appsec_authz_endpoint_is_static_asset(endpoint) {
        return None;
    }
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .unwrap_or("GET")
        .to_ascii_uppercase();
    if !matches!(method.as_str(), "GET" | "HEAD" | "OPTIONS") {
        return None;
    }
    let object_ref = value
        .get("object")
        .or_else(|| value.get("object_ref"))
        .or_else(|| value.get("object_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty());
    let owner_object_refs = appsec_authz_string_array(
        value
            .get("owner_object_refs")
            .or_else(|| value.get("object_refs")),
    );
    let owner_subject = value
        .get("owner_subject")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty());
    let owner_body_hash = value
        .get("owner_body_hash")
        .or_else(|| value.get("body_hash"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty());
    let explicit_expected = value
        .get("expected")
        .and_then(Value::as_str)
        .map(str::trim)
        .map(|item| item.to_ascii_lowercase())
        .is_some_and(|item| matches!(item.as_str(), "allow" | "deny"));
    let authz_scoped = explicit_expected
        || object_ref.is_some()
        || !owner_object_refs.is_empty()
        || (owner_subject.is_some() && owner_body_hash.is_some());
    if !authz_scoped {
        return None;
    }
    let mut candidate = json!({
        "endpoint": endpoint,
        "method": method,
        "expected": value.get("expected").and_then(Value::as_str).unwrap_or("deny"),
    });
    if let Some(object) = candidate.as_object_mut() {
        for key in [
            "object",
            "object_ref",
            "object_type",
            "object_source",
            "owner_subject",
            "owner_body_hash",
            "owner_body_length",
            "body_class",
        ] {
            if let Some(value) = value.get(key).cloned() {
                object.insert(key.to_string(), value);
            }
        }
        if !owner_object_refs.is_empty() {
            object.insert("owner_object_refs".to_string(), json!(owner_object_refs));
        }
    }
    Some(candidate)
}

fn appsec_authz_endpoint_is_static_asset(endpoint: &str) -> bool {
    let path = endpoint
        .split(['?', '#'])
        .next()
        .unwrap_or(endpoint)
        .to_ascii_lowercase();
    [
        ".js", ".mjs", ".css", ".map", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".webp",
        ".avif", ".woff", ".woff2", ".ttf", ".eot", ".mp4", ".webm", ".pdf", ".zip",
    ]
    .iter()
    .any(|suffix| path.ends_with(suffix))
}

fn appsec_authz_string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn execute_appsec_business_os_cli_command(root: &Path, args: &[String]) -> anyhow::Result<Value> {
    if args.first().map(String::as_str) != Some("web-stack") {
        anyhow::bail!("AppSec worker only allows ctox business-os web-stack commands");
    }
    crate::service::business_os::run_business_os_web_stack_cli_json(root, &args[1..])
}

fn parse_optional_u64_arg(args: &[String], flag: &str) -> anyhow::Result<Option<u64>> {
    arg_value(args, flag)
        .map(|value| {
            value
                .parse::<u64>()
                .with_context(|| format!("failed to parse {flag}"))
        })
        .transpose()
}

fn appsec_reusable_expected_artifact_output(
    state_dir: &Path,
    command: &Value,
) -> anyhow::Result<Option<(Value, Option<String>)>> {
    if !appsec_command_can_reuse_expected_artifact(command) {
        return Ok(None);
    }
    let Some(expected) = command
        .get("expected_artifact")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let artifact_path = appsec_expected_artifact_path(state_dir, expected);
    if !artifact_path.is_file() {
        return Ok(None);
    }
    let artifact: Value = serde_json::from_slice(&fs::read(&artifact_path).with_context(|| {
        format!(
            "failed to read existing AppSec expected artifact {}",
            artifact_path.display()
        )
    })?)
    .with_context(|| {
        format!(
            "failed to parse existing AppSec expected artifact {}",
            artifact_path.display()
        )
    })?;
    if appsec_value_contains_forbidden_authz_evidence_key(&artifact) {
        return Ok(Some((
            json!({
                "ok": false,
                "status": "blocked-secret-artifact",
                "artifact": artifact_path.to_string_lossy(),
                "reused_existing_artifact": true,
                "error": "existing expected AppSec artifact contains secret-like material keys",
            }),
            Some(artifact_path.to_string_lossy().to_string()),
        )));
    }
    let ok = appsec_artifact_bool(&artifact, "ok").unwrap_or(true);
    let status = appsec_artifact_string(&artifact, "status")
        .unwrap_or_else(|| if ok { "completed" } else { "failed" }.to_string());
    Ok(Some((
        json!({
            "ok": ok,
            "status": status,
            "artifact": artifact_path.to_string_lossy(),
            "reused_existing_artifact": true,
            "result": artifact,
        }),
        Some(artifact_path.to_string_lossy().to_string()),
    )))
}

fn appsec_command_can_reuse_expected_artifact(command: &Value) -> bool {
    if command.pointer("/produces/artifact/placeholder").is_some() {
        return false;
    }
    matches!(
        command.get("tool").and_then(Value::as_str),
        Some(
            "ctox_web_auth_assist_request"
                | "ctox_web_auth_assist_signup"
                | "ctox_web_auth_assist_login"
                | "ctox_browser_automation"
                | "ctox_browser_context_capture"
                | "ctox_browser_context_extract"
        )
    )
}

fn appsec_artifact_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool).or_else(|| {
        value
            .get("result")
            .and_then(|result| result.get(key))
            .and_then(Value::as_bool)
    })
}

fn appsec_artifact_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("result")
                .and_then(|result| result.get(key))
                .and_then(Value::as_str)
        })
        .map(ToString::to_string)
}

fn appsec_value_contains_forbidden_authz_evidence_key(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, child)| {
            appsec_authz_evidence_forbidden_key(key)
                || appsec_value_contains_forbidden_authz_evidence_key(child)
        }),
        Value::Array(items) => items
            .iter()
            .any(appsec_value_contains_forbidden_authz_evidence_key),
        _ => false,
    }
}

fn appsec_stage_command_plan(stage: &Value) -> Value {
    Value::Array(
        stage
            .get("run_commands")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    )
}

fn persist_appsec_command_expected_artifact(
    state_dir: &Path,
    command: &Value,
    output: &Value,
) -> anyhow::Result<Option<String>> {
    let Some(expected) = command
        .get("expected_artifact")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let artifact_path = appsec_expected_artifact_path(state_dir, expected);
    if let Some(parent) = artifact_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create appsec artifact dir {}", parent.display())
        })?;
    }
    let artifact = appsec_redacted_command_artifact(command, output, &artifact_path);
    fs::write(&artifact_path, serde_json::to_vec_pretty(&artifact)?).with_context(|| {
        format!(
            "failed to write appsec artifact {}",
            artifact_path.display()
        )
    })?;
    Ok(Some(artifact_path.to_string_lossy().to_string()))
}

fn appsec_expected_artifact_path(state_dir: &Path, expected: &str) -> PathBuf {
    let path = PathBuf::from(expected);
    if path.is_absolute() {
        path
    } else {
        state_dir.join(path)
    }
}

fn appsec_redacted_command_artifact(
    command: &Value,
    output: &Value,
    artifact_path: &Path,
) -> Value {
    let result = output
        .get("result")
        .cloned()
        .unwrap_or_else(|| output.clone());
    let mut artifact = json!({
        "version": "ctox.appsec_pentest.web_stack_evidence.v1",
        "source_tool": command.get("tool").cloned().unwrap_or(Value::Null),
        "phase": command.get("phase").cloned().unwrap_or(Value::Null),
        "target": command.get("target").cloned().unwrap_or(Value::Null),
        "subject_id": command.get("subject_id").cloned().unwrap_or(Value::Null),
        "owner_subject": command.get("owner_subject").cloned().unwrap_or(Value::Null),
        "actor_subject": command.get("actor_subject").cloned().unwrap_or(Value::Null),
        "artifact": artifact_path.to_string_lossy(),
        "result": result,
        "secret_policy": "redacted: no cookies, tokens, passwords, screenshots, raw browser streams, or storage state",
    });
    for key in [
        "objects",
        "cases",
        "product_failures",
        "api_requests",
        "performance_requests",
        "visible_links",
        "replay_candidates",
        "replay_candidates_required",
    ] {
        if let Some(value) = artifact.pointer(&format!("/result/{key}")).cloned() {
            if let Some(object) = artifact.as_object_mut() {
                object.insert(key.to_string(), value);
            }
        }
    }
    appsec_redact_for_authz_evidence(&mut artifact);
    artifact
}

fn appsec_redact_for_authz_evidence(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let keys = object.keys().cloned().collect::<Vec<_>>();
            for key in keys {
                if appsec_authz_evidence_forbidden_key(&key) {
                    object.remove(&key);
                    continue;
                }
                if let Some(child) = object.get_mut(&key) {
                    appsec_redact_for_authz_evidence(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                appsec_redact_for_authz_evidence(item);
            }
        }
        _ => {}
    }
}

fn appsec_authz_evidence_forbidden_key(key: &str) -> bool {
    let key_lc = key.to_ascii_lowercase();
    if matches!(
        key_lc.as_str(),
        "secret_policy" | "secret_redaction" | "redaction" | "redacted"
    ) {
        return false;
    }
    let compact = key_lc.replace(['_', '-'], "");
    [
        "password",
        "passwd",
        "pwd",
        "cookie",
        "cookies",
        "token",
        "accesstoken",
        "refreshtoken",
        "authorization",
        "bearer",
        "privatekey",
        "apikey",
        "screenshot",
        "rawbrowserstream",
        "browserstream",
        "storagestate",
        "secret",
    ]
    .iter()
    .any(|needle| key_lc.contains(needle) || compact.contains(needle))
}

fn completed_run_artifacts(commands: &[Value]) -> Vec<String> {
    commands
        .iter()
        .filter(|command| command.get("ok").and_then(Value::as_bool) == Some(true))
        .filter_map(|command| {
            command
                .pointer("/output/run/combined_artifact")
                .and_then(Value::as_str)
                .or_else(|| command.pointer("/output/artifact").and_then(Value::as_str))
                .or_else(|| {
                    command
                        .pointer("/output/import_result/artifact")
                        .and_then(Value::as_str)
                })
                .or_else(|| command.get("artifact_path").and_then(Value::as_str))
                .map(ToString::to_string)
        })
        .collect()
}

fn mark_appsec_stage_coverage(
    root: &Path,
    state_dir: &Path,
    stage: &Value,
    artifacts: &[String],
) -> anyhow::Result<Value> {
    let phase = stage
        .get("phase")
        .and_then(Value::as_str)
        .context("AppSec stage is missing phase")?;
    let target = stage
        .get("target")
        .and_then(Value::as_str)
        .context("AppSec stage is missing target")?;
    let mut args = vec![
        "coverage".to_string(),
        "mark".to_string(),
        "--state-dir".to_string(),
        state_dir.to_string_lossy().to_string(),
        "--phase".to_string(),
        phase.to_string(),
        "--target".to_string(),
        target.to_string(),
        "--status".to_string(),
        "completed".to_string(),
        "--note".to_string(),
        format!(
            "ctox appsec pipeline worker completed stage `{}` with {} scanner artifact(s)",
            appsec_stage_id(stage).unwrap_or("unknown-stage"),
            artifacts.len()
        ),
    ];
    for artifact in artifacts {
        args.push("--artifact".to_string());
        args.push(artifact.clone());
    }
    run_projected_appsec_command(root, &args)
}

fn appsec_pipeline_stage_terminal(status_output: &Value, stage: &Value) -> bool {
    let stage_id = appsec_stage_id(stage);
    status_output
        .pointer("/pipeline_status/stages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|candidate| {
            candidate.get("id").and_then(Value::as_str) == stage_id
                && matches!(
                    candidate.get("status").and_then(Value::as_str),
                    Some("completed" | "not-applicable")
                )
        })
}

fn appsec_stage_id(stage: &Value) -> Option<&str> {
    stage.get("id").and_then(Value::as_str)
}

fn appsec_stage_summary(stage: &Value) -> Value {
    json!({
        "id": stage.get("id").cloned().unwrap_or(Value::Null),
        "phase": stage.get("phase").cloned().unwrap_or(Value::Null),
        "target": stage.get("target").cloned().unwrap_or(Value::Null),
        "status": stage.get("status").cloned().unwrap_or(Value::Null),
        "active_required": stage.get("active_required").cloned().unwrap_or(Value::Null),
    })
}

fn appsec_stage_worker_blocker_note(
    command_failures: usize,
    command_blocks: usize,
    completed_artifacts: &[String],
) -> String {
    if command_failures > 0 {
        return format!(
            "AppSec pipeline worker failed {command_failures} command(s); inspect appsec_worker_result"
        );
    }
    if command_blocks > 0 {
        return format!(
            "AppSec pipeline worker blocked on {command_blocks} command(s) that require external evidence, placeholders, or unsupported tooling"
        );
    }
    if completed_artifacts.is_empty() {
        return "AppSec pipeline worker produced no completed run artifact".to_string();
    }
    "AppSec pipeline worker could not close the stage".to_string()
}

fn enqueue_appsec_pipeline_queue_tasks(
    root: &Path,
    output: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let queue_tasks = output
        .pointer("/queue_spec/queue_tasks")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut enqueued = Vec::new();
    for spec in queue_tasks {
        let title = spec
            .get("title")
            .and_then(serde_json::Value::as_str)
            .context("AppSec queue task spec is missing title")?;
        let prompt = spec
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .context("AppSec queue task spec is missing prompt")?;
        let thread_key = spec
            .get("thread_key")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("appsec-pipeline")
            .to_string();
        let workspace_root = spec
            .get("workspace_root")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);
        let priority = spec
            .get("priority")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("normal")
            .to_string();
        let suggested_skill = spec
            .get("skill")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);
        let extra_metadata = spec
            .get("metadata")
            .cloned()
            .or_else(|| {
                spec.get("idempotency_key").map(|key| {
                    serde_json::json!({
                        "source": "ctox-appsec-pipeline",
                        "idempotency_key": key,
                    })
                })
            })
            .or_else(|| {
                Some(serde_json::json!({
                    "source": "ctox-appsec-pipeline",
                }))
            });
        let task = channels::create_queue_task(
            root,
            channels::QueueTaskCreateRequest {
                title: title.to_string(),
                prompt: prompt.to_string(),
                thread_key,
                workspace_root,
                priority,
                suggested_skill,
                parent_message_key: None,
                extra_metadata,
            },
        )?;
        enqueued.push(serde_json::json!({
            "stage_id": spec.get("stage_id").cloned().unwrap_or(serde_json::Value::Null),
            "phase": spec.get("phase").cloned().unwrap_or(serde_json::Value::Null),
            "target": spec.get("target").cloned().unwrap_or(serde_json::Value::Null),
            "message_key": task.message_key,
            "thread_key": task.thread_key,
            "route_status": task.route_status,
            "priority": task.priority,
        }));
    }
    Ok(serde_json::json!({
        "version": "ctox.appsec.pipeline_enqueue.v1",
        "created_or_updated": enqueued.len(),
        "tasks": enqueued,
        "idempotent": true,
    }))
}

const CHAT_USAGE: &str = "usage: ctox chat \"<instruction>\" [--thread-key <key>] [--workspace <path>] [--wait] [--timeout-secs <n>] [--atif-out <path>] [--to <addr> ...] [--cc <addr> ...] [--subject <text>] [--attach-file <path> ...]";

const SKILLS_USAGE: &str = "usage:
  ctox skills system list
  ctox skills system show <name> [--body]
  ctox skills system diff
  ctox skills system migrate
  ctox skills system export <name> --target <dir>
  ctox skills packs list
  ctox skills packs install <name>
  ctox skills user list
  ctox skills user path
  ctox skills user create --name <name> --description <text> [--body <text>] [--overwrite]
  ctox skills user update --name <name> --description <text> [--body <text>] [--overwrite]";

fn handle_skills_command(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("system") => handle_system_skills_command(root, &args[1..]),
        Some("packs") => handle_skill_packs_command(root, &args[1..]),
        Some("user") => handle_user_skills_command(root, &args[1..]),
        Some("list") => {
            let system = skill_store::list_system_skill_bundles(root)?;
            let user = skill_store::list_user_skill_bundles(root)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "system_count": system.len(),
                    "user_count": user.len(),
                    "system": system,
                    "user": user
                }))?
            );
            Ok(())
        }
        Some("--help") | Some("-h") | None => {
            println!("{SKILLS_USAGE}");
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown skills subcommand `{other}`\n{SKILLS_USAGE}"),
    }
}

fn handle_system_skills_command(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("list") => {
            let skills = skill_store::list_system_skill_bundles(root)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "count": skills.len(),
                    "skills": skills
                }))?
            );
            Ok(())
        }
        Some("show") => {
            let name = args
                .get(1)
                .context("usage: ctox skills system show <name> [--body]")?;
            skill_store::bootstrap_embedded_system_skills(root)?;
            let skills = skill_store::list_system_skill_bundles(root)?;
            let skill = skills
                .into_iter()
                .find(|skill| skill.skill_name == *name || skill.skill_id == *name)
                .with_context(|| format!("system skill not found: {name}"))?;
            let include_body = args.iter().any(|arg| arg == "--body");
            let body = if include_body {
                skill_store::load_skill_body_by_name(root, &skill.skill_name)?
            } else {
                None
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "skill": skill,
                    "body": body
                }))?
            );
            Ok(())
        }
        Some("diff") => {
            let diff = skill_store::diff_embedded_system_skills(root)?;
            let changed = diff
                .iter()
                .filter(|item| item.status != "unchanged")
                .count();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": changed == 0,
                    "changed_count": changed,
                    "count": diff.len(),
                    "diff": diff
                }))?
            );
            Ok(())
        }
        Some("migrate") => {
            let report = skill_store::migrate_skill_store(root)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "migration": report
                }))?
            );
            Ok(())
        }
        Some("export") => {
            let name = args
                .get(1)
                .context("usage: ctox skills system export <name> --target <dir>")?;
            let target = find_flag_value(args, "--target")
                .map(PathBuf::from)
                .context("usage: ctox skills system export <name> --target <dir>")?;
            let path = skill_store::export_system_skill(root, name, &target)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "path": path
                }))?
            );
            Ok(())
        }
        Some("--help") | Some("-h") | None => {
            println!("{SKILLS_USAGE}");
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown skills system subcommand `{other}`\n{SKILLS_USAGE}"),
    }
}

fn handle_skill_packs_command(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("list") => {
            let packs = skill_store::source_pack_names(root)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "count": packs.len(),
                    "packs": packs
                }))?
            );
            Ok(())
        }
        Some("install") => {
            let name = args
                .get(1)
                .context("usage: ctox skills packs install <name>")?;
            let path = skill_store::install_source_pack(root, name)?;
            skill_store::bootstrap_from_roots(root)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "name": name,
                    "path": path
                }))?
            );
            Ok(())
        }
        Some("--help") | Some("-h") | None => {
            println!("{SKILLS_USAGE}");
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown skills packs subcommand `{other}`\n{SKILLS_USAGE}"),
    }
}

fn handle_user_skills_command(root: &Path, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("list") => {
            let skills = skill_store::list_user_skill_bundles(root)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "count": skills.len(),
                    "skills": skills
                }))?
            );
            Ok(())
        }
        Some("path") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "codex_home_skills": skill_store::codex_home_skills_root(),
                    "runtime_skills": skill_store::runtime_user_skill_root(root)
                }))?
            );
            Ok(())
        }
        Some("create") | Some("update") => {
            let name = find_flag_value(args, "--name")
                .context("usage: ctox skills user create --name <name> --description <text> [--body <text>] [--overwrite]")?;
            let description = find_flag_value(args, "--description").unwrap_or("User skill");
            let body = find_flag_value(args, "--body")
                .unwrap_or("# Instructions\n\nAdd workflow-specific instructions here.");
            let overwrite = args.iter().any(|arg| arg == "--overwrite")
                || args.first().map(String::as_str) == Some("update");
            let path =
                skill_store::create_or_update_user_skill(name, description, body, overwrite)?;
            skill_store::bootstrap_from_roots(root)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "name": name,
                    "path": path
                }))?
            );
            Ok(())
        }
        Some("--help") | Some("-h") | None => {
            println!("{SKILLS_USAGE}");
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown skills user subcommand `{other}`\n{SKILLS_USAGE}"),
    }
}

fn handle_chat(root: &Path, args: &[String]) -> anyhow::Result<()> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{CHAT_USAGE}");
        return Ok(());
    }

    let wait = args.iter().any(|arg| arg == "--wait");
    let thread_key = find_flag_value(args, "--thread-key").map(str::to_owned);
    let workspace = find_flag_value(args, "--workspace").map(PathBuf::from);
    let atif_out = find_flag_value(args, "--atif-out").map(PathBuf::from);
    let timeout_secs = find_flag_value(args, "--timeout-secs")
        .map(|value| value.parse::<u64>())
        .transpose()
        .context("failed to parse --timeout-secs")?
        .unwrap_or(1800);
    if atif_out.is_some() && !wait {
        anyhow::bail!("--atif-out requires --wait");
    }

    // Repeatable outbound-mail flags: --to/--cc collect every occurrence;
    // --subject is a single string. When any --to is present we synthesize an
    // explicit `OutboundEmailIntent` and attach it to the chat submission.
    let to_recipients: Vec<String> = collect_flag_values(args, "--to");
    let cc_recipients: Vec<String> = collect_flag_values(args, "--cc");
    let subject = find_flag_value(args, "--subject").map(str::to_owned);
    let attachments = resolve_chat_attachment_paths(args)?;
    if !cc_recipients.is_empty() && to_recipients.is_empty() {
        anyhow::bail!("--cc requires at least one --to recipient");
    }
    if subject.is_some() && to_recipients.is_empty() {
        anyhow::bail!("--subject requires at least one --to recipient");
    }
    if !attachments.is_empty() && to_recipients.is_empty() {
        anyhow::bail!("--attach-file requires at least one --to recipient");
    }

    let mut prompt_parts = Vec::new();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--wait" => idx += 1,
            "--thread-key" | "--workspace" | "--atif-out" | "--timeout-secs" | "--to" | "--cc"
            | "--subject" | "--attach-file" => {
                idx += 2;
            }
            value => {
                prompt_parts.push(value);
                idx += 1;
            }
        }
    }
    let raw_prompt = prompt_parts.join(" ");
    let prompt = build_chat_prompt(raw_prompt.trim(), workspace.as_deref())?;
    let prompt = service::prepare_chat_prompt(root, &prompt)?.prompt;

    let status = service::service_status_snapshot(root)?;
    if !status.running {
        anyhow::bail!(
            "CTOX service is not running. Start it with `ctox start` or `ctox service --foreground`."
        );
    }

    let outbound_email = if to_recipients.is_empty() {
        None
    } else {
        let account_key = channels::default_email_account_key(root)
            .context("--to provided but no default email account is configured")?;
        let intent_thread_key = thread_key
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "chat-outbound".to_string());
        let intent_subject = subject
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Update".to_string());
        Some(service::OutboundEmailIntent {
            account_key,
            thread_key: intent_thread_key,
            subject: intent_subject,
            to: to_recipients,
            cc: cc_recipients,
            attachments,
        })
    };
    let outbound_email_for_wait = outbound_email.clone();
    let outbound_terminal_count_before = outbound_email_for_wait
        .as_ref()
        .map(|intent| {
            let action = channels::FounderOutboundAction::from(intent.clone());
            channels::terminal_founder_outbound_artifact_count(root, &action)
        })
        .transpose()?;
    let last_completed_before_submit = status.last_completed_at.clone();

    service::submit_chat_prompt_with_intent(root, &prompt, thread_key.as_deref(), outbound_email)?;

    let conversation_id =
        inference::turn_loop::conversation_id_for_thread_key(thread_key.as_deref());
    let mut final_status = None;
    if wait {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
        let wait_probe = service::StatusProbeOptions {
            reconcile_runtime_switch: false,
            systemd_cache_ttl: Some(std::time::Duration::from_secs(5)),
            manager_probe: false,
            status_ipc_timeout: Some(std::time::Duration::from_secs(1)),
            lifecycle_alerts: false,
            include_business_os: false,
        };
        loop {
            let status = service::service_status_snapshot_with(root, &wait_probe)?;
            if !status.running {
                anyhow::bail!("CTOX service stopped before the chat request completed");
            }
            let completed_after_submit = chat_status_has_completed_since(
                status.last_completed_at.as_ref(),
                last_completed_before_submit.as_ref(),
            );
            let outbound_completed = outbound_email_for_wait
                .as_ref()
                .zip(outbound_terminal_count_before)
                .map(|(intent, before_count)| {
                    let action = channels::FounderOutboundAction::from(intent.clone());
                    channels::terminal_founder_outbound_artifact_count(root, &action)
                        .map(|after_count| after_count > before_count)
                })
                .transpose()?
                .unwrap_or(false);
            if completed_after_submit || outbound_completed {
                final_status = Some(status);
                break;
            }
            if std::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "timed out waiting for CTOX to finish the chat request after {}s",
                    timeout_secs
                );
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        let completed_after_submit = final_status
            .as_ref()
            .and_then(|status| status.last_completed_at.as_ref())
            .is_some_and(|completed| {
                chat_status_has_completed_since(
                    Some(completed),
                    last_completed_before_submit.as_ref(),
                )
            });
        let outbound_completed_after = if let (Some(intent), Some(before_count)) = (
            outbound_email_for_wait.as_ref(),
            outbound_terminal_count_before,
        ) {
            let action = channels::FounderOutboundAction::from(intent.clone());
            channels::terminal_founder_outbound_artifact_count(root, &action)? > before_count
        } else {
            false
        };
        if !completed_after_submit && !outbound_completed_after {
            anyhow::bail!(
                "CTOX service became idle before this chat request reported a completed turn"
            );
        }
        if let (Some(intent), Some(before_count)) = (
            outbound_email_for_wait.as_ref(),
            outbound_terminal_count_before,
        ) {
            let action = channels::FounderOutboundAction::from(intent.clone());
            let after_count = channels::terminal_founder_outbound_artifact_count(root, &action)?;
            if after_count <= before_count {
                anyhow::bail!(
                    "CTOX chat finished without a new accepted outbound email artifact for subject {:?} to {:?}; the agent did not complete the reviewed send after review approval",
                    action.subject,
                    action.to
                );
            }
        }
    }

    if let Some(out_path) = atif_out.as_deref() {
        let settings = runtime_env::effective_runtime_env_map(root).unwrap_or_default();
        let model_name = runtime_env::effective_chat_model_from_map(&settings)
            .unwrap_or_else(|| model_registry::default_local_chat_model().to_string());
        let session_id = thread_key
            .clone()
            .unwrap_or_else(|| format!("chat-{}", conversation_id));
        let notes = final_status
            .as_ref()
            .and_then(|status| status.last_error.clone())
            .map(|err| format!("service reported last_error: {err}"));
        let trajectory = export::atif::build_trajectory(
            &root.join("runtime/ctox.sqlite3"),
            conversation_id,
            &session_id,
            "ctox",
            env!("CTOX_BUILD_VERSION"),
            &model_name,
            notes,
        )?;
        export::atif::write_trajectory(&trajectory, out_path).with_context(|| {
            format!("failed to write ATIF trajectory to {}", out_path.display())
        })?;
    }

    let last_error = final_status
        .as_ref()
        .and_then(|status| status.last_error.clone());
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "status": if wait { "completed" } else { "submitted" },
            "conversation_id": conversation_id,
            "thread_key": thread_key,
            "waited": wait,
            "last_error": last_error,
        }))?
    );

    if let Some(err) = last_error {
        anyhow::bail!(err);
    }
    Ok(())
}

fn handle_native_qwen3_embedding_service(args: &[String]) -> anyhow::Result<()> {
    let transport = find_flag_value(args, "--transport").context(
        "usage: ctox __native-qwen3-embedding-service --transport <ipc-endpoint> [--compute-target gpu|cpu]",
    )?;
    let compute_target =
        parse_compute_target_value(find_flag_value(args, "--compute-target").unwrap_or("gpu"))?;
    native_embedding::serve_socket(native_embedding::NativeEmbeddingLaunch {
        transport: inference::local_transport::LocalTransport::from_ipc_endpoint_string(transport),
        compute_target,
    })
}

fn handle_native_voxtral_stt_service(args: &[String], root: &Path) -> anyhow::Result<()> {
    let transport = find_flag_value(args, "--transport").context(
        "usage: ctox __native-voxtral-stt-service --transport <ipc-endpoint> [--compute-target gpu|cpu] [--model-path <path>]",
    )?;
    let compute_target =
        parse_compute_target_value(find_flag_value(args, "--compute-target").unwrap_or("gpu"))?;
    let model_path = find_flag_value(args, "--model-path")
        .map(PathBuf::from)
        .or_else(|| native_stt::configured_or_default_model_path(root));
    native_stt::serve_socket(native_stt::NativeSttLaunch {
        transport: inference::local_transport::LocalTransport::from_ipc_endpoint_string(transport),
        compute_target,
        model_path,
        root: root.to_path_buf(),
    })
}

fn handle_native_voxtral_tts_service(args: &[String], root: &Path) -> anyhow::Result<()> {
    let transport = find_flag_value(args, "--transport").context(
        "usage: ctox __native-voxtral-tts-service --transport <ipc-endpoint> [--compute-target gpu|cpu] [--model-dir <path>]",
    )?;
    let compute_target =
        parse_compute_target_value(find_flag_value(args, "--compute-target").unwrap_or("gpu"))?;
    let model_dir = find_flag_value(args, "--model-dir")
        .map(PathBuf::from)
        .or_else(|| native_tts::configured_or_default_model_dir(root));
    native_tts::serve_socket(native_tts::NativeTtsLaunch {
        transport: inference::local_transport::LocalTransport::from_ipc_endpoint_string(transport),
        compute_target,
        model_dir,
    })
}

fn parse_compute_target_value(value: &str) -> anyhow::Result<engine::ComputeTarget> {
    match value.trim().to_ascii_lowercase().as_str() {
        "gpu" => Ok(engine::ComputeTarget::Gpu),
        "cpu" => Ok(engine::ComputeTarget::Cpu),
        other => anyhow::bail!("unsupported compute target `{other}`; expected gpu or cpu"),
    }
}

fn build_chat_prompt(raw_prompt: &str, workspace: Option<&Path>) -> anyhow::Result<String> {
    if raw_prompt.is_empty() {
        anyhow::bail!(CHAT_USAGE);
    }
    if raw_prompt.starts_with("Work only inside this workspace:") || workspace.is_none() {
        return Ok(raw_prompt.to_string());
    }
    let workspace = workspace.expect("workspace checked above");
    Ok(format!(
        "Work only inside this workspace:\n{}\n\n{}",
        workspace.display(),
        raw_prompt
    ))
}

/// `ctox continuity-update` — tool-based continuity refresh primitive.
///
/// The continuity refresh used to require the model to emit a textual
/// `+`/`-` diff in a strict format. Measurement across Terminal-Bench-2
/// showed that format was model-bound: MiniMax-M2.7 produced 0 parseable
/// diffs across 21 refresh attempts, and gpt-5.4 produced diffs that
/// always needed a canonicalization fix-up. Switching to structured tool
/// calls removes the parse layer entirely: the model picks one of three
/// modes depending on the size of the change.
///
/// Usage:
///   ctox continuity-update --db <path> --conversation-id <id> --kind <narrative|anchors|focus> --mode full
///       -- the new document body is read from stdin
///   ctox continuity-update --db <path> --conversation-id <id> --kind <kind> --mode replace --find <text> --replace <text>
///   ctox continuity-update --db <path> --conversation-id <id> --kind <kind> --mode diff
///       -- the `+`/`-` diff is read from stdin (legacy path)
fn handle_continuity_update(args: &[String]) -> anyhow::Result<()> {
    // Default to the conventional runtime DB if --db is not supplied. Codex-
    // exec children inherit CTOX_ROOT so this usually just works.
    let ctox_root_env = std::env::var("CTOX_ROOT").ok();
    let default_db = ctox_root_env
        .as_deref()
        .map(|r| format!("{}/runtime/ctox.sqlite3", r.trim_end_matches('/')));
    let db_path = find_flag_value(args, "--db")
        .map(str::to_string)
        .or(default_db)
        .context("usage: ctox continuity-update --conversation-id <id> --kind <narrative|anchors|focus> --mode <full|replace|diff> [--db <path>] [--find <text>] [--replace <text>]")?;
    let conversation_id: i64 = find_flag_value(args, "--conversation-id")
        .context("missing required --conversation-id")?
        .parse()
        .context("failed to parse --conversation-id")?;
    let kind =
        find_flag_value(args, "--kind").context("missing --kind (narrative|anchors|focus)")?;
    let mode = find_flag_value(args, "--mode").context("missing --mode (full|replace|diff)")?;
    let db = Path::new(&db_path);
    let result = match mode {
        "full" => {
            let mut content = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut content)
                .context("failed to read continuity document body from stdin")?;
            context::lcm::run_continuity_full_replace(db, conversation_id, kind, &content)?
        }
        "replace" => {
            let find =
                find_flag_value(args, "--find").context("--mode replace requires --find <text>")?;
            let replace = find_flag_value(args, "--replace").unwrap_or("");
            context::lcm::run_continuity_string_replace(db, conversation_id, kind, find, replace)?
        }
        "diff" => {
            let mut diff = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut diff)
                .context("failed to read continuity diff from stdin")?;
            let engine = context::lcm::LcmEngine::open(db, context::lcm::LcmConfig::default())?;
            engine.continuity_apply_diff(
                conversation_id,
                context::lcm::ContinuityKind::parse(kind)?,
                &diff,
            )?
        }
        other => anyhow::bail!("unknown continuity-update mode: {other}"),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn resolve_workspace_root() -> anyhow::Result<PathBuf> {
    if let Some(root) = validated_workspace_root_override("CTOX_ROOT") {
        return Ok(root);
    }
    let current_dir = std::env::current_dir().context("failed to resolve CTOX workspace root")?;
    if let Some(root) = find_ctox_root_from_ancestors(&current_dir) {
        return Ok(root);
    }
    if let Some(root) = validated_workspace_root_override("CTOX_HOME") {
        return Ok(root);
    }
    let current_exe = std::env::current_exe().ok();
    if let Some(root) = current_exe
        .as_deref()
        .and_then(|exe| resolve_runtime_ctox_root(exe, home_dir().as_deref()))
    {
        return Ok(root);
    }
    Ok(current_dir)
}

fn openrouter_tool_smoke_json(root: &Path, args: &[String]) -> anyhow::Result<serde_json::Value> {
    let model = find_flag_value(args, "--model").unwrap_or("deepseek/deepseek-v4-flash");
    let requested_tool_choice = find_flag_value(args, "--tool-choice").unwrap_or("all");
    let api_key = std::env::var("OPENROUTER_API_KEY")
        .ok()
        .or_else(|| secrets::get_credential(root, "OPENROUTER_API_KEY"))
        .context(
            "OPENROUTER_API_KEY not found in env or CTOX secret store credentials/OPENROUTER_API_KEY",
        )?;
    let variants = openrouter_tool_smoke_variants(requested_tool_choice)?;
    let endpoint = "https://openrouter.ai/api/v1/chat/completions";
    let mut results = Vec::new();
    for (variant_name, tool_choice) in variants {
        let payload = serde_json::json!({
            "model": model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are testing tool calling. If tools are available, respond only by calling the requested tool."
                },
                {
                    "role": "user",
                    "content": "Call the record_status tool with status exactly ok. Do not answer in prose."
                }
            ],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "record_status",
                    "description": "Record a status string.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "status": {"type": "string"}
                        },
                        "required": ["status"],
                        "additionalProperties": false
                    }
                }
            }],
            "tool_choice": tool_choice,
            "max_tokens": 128,
            "temperature": 0,
            "stream": false
        });
        let payload = serde_json::to_string(&payload)?;
        let request_result = ureq::post(endpoint)
            .set("Authorization", &format!("Bearer {api_key}"))
            .set("Content-Type", "application/json")
            .set("HTTP-Referer", "https://ctox.local")
            .set("X-OpenRouter-Title", "ctox-openrouter-tool-smoke")
            .timeout(std::time::Duration::from_secs(120))
            .send_string(&payload);
        let (status, body) = match request_result {
            Ok(response) => (response.status(), response.into_string()?),
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                (status, body)
            }
            Err(err) => {
                results.push(serde_json::json!({
                    "variant": variant_name,
                    "transport_error": err.to_string(),
                    "has_tool_calls": false
                }));
                continue;
            }
        };
        let parsed = serde_json::from_str::<serde_json::Value>(&body).unwrap_or_else(|_| {
            serde_json::json!({
                "error": {
                    "message": format!("non-json response ({} bytes)", body.len())
                }
            })
        });
        results.push(openrouter_tool_smoke_summary(variant_name, status, &parsed));
    }
    let ok = results.iter().all(|item| {
        item.get("has_tool_calls")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    });
    Ok(serde_json::json!({
        "ok": ok,
        "endpoint": endpoint,
        "model": model,
        "checked_variants": results.len(),
        "results": results
    }))
}

fn openrouter_tool_smoke_variants(
    requested_tool_choice: &str,
) -> anyhow::Result<Vec<(&'static str, serde_json::Value)>> {
    let named = serde_json::json!({
        "type": "function",
        "function": {"name": "record_status"}
    });
    match requested_tool_choice {
        "all" => Ok(vec![
            ("auto", serde_json::json!("auto")),
            ("required", serde_json::json!("required")),
            ("named", named),
        ]),
        "auto" => Ok(vec![("auto", serde_json::json!("auto"))]),
        "required" => Ok(vec![("required", serde_json::json!("required"))]),
        "named" => Ok(vec![("named", named)]),
        other => anyhow::bail!(
            "unsupported --tool-choice {other:?}; expected auto, required, named, or all"
        ),
    }
}

fn openrouter_tool_smoke_summary(
    variant_name: &'static str,
    status: u16,
    payload: &serde_json::Value,
) -> serde_json::Value {
    let choice = payload
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .and_then(|choices| choices.first())
        .unwrap_or(&serde_json::Value::Null);
    let message = choice.get("message").and_then(serde_json::Value::as_object);
    let tool_calls = message
        .and_then(|message| message.get("tool_calls"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tool_call_names = tool_calls
        .iter()
        .map(|tool_call| {
            tool_call
                .get("function")
                .and_then(|function| function.get("name"))
                .or_else(|| tool_call.get("name"))
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        })
        .collect::<Vec<_>>();
    let tool_call_arguments = tool_calls
        .iter()
        .map(|tool_call| {
            tool_call
                .get("function")
                .and_then(|function| function.get("arguments"))
                .or_else(|| tool_call.get("arguments"))
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        })
        .collect::<Vec<_>>();
    let content_len = message
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str)
        .map(str::len)
        .unwrap_or(0);
    let error = payload.get("error");
    serde_json::json!({
        "variant": variant_name,
        "status": status,
        "response_model": payload.get("model").cloned().unwrap_or(serde_json::Value::Null),
        "provider": payload.get("provider").cloned().unwrap_or(serde_json::Value::Null),
        "finish_reason": choice.get("finish_reason").cloned().unwrap_or(serde_json::Value::Null),
        "native_finish_reason": choice.get("native_finish_reason").cloned().unwrap_or(serde_json::Value::Null),
        "has_tool_calls": !tool_calls.is_empty(),
        "tool_call_count": tool_calls.len(),
        "tool_call_names": tool_call_names,
        "tool_call_arguments": tool_call_arguments,
        "content_len": content_len,
        "error_code": error.and_then(|error| error.get("code")).cloned().unwrap_or(serde_json::Value::Null),
        "error_message": error.and_then(|error| error.get("message")).cloned().unwrap_or(serde_json::Value::Null)
    })
}

fn validated_workspace_root_override(key: &str) -> Option<PathBuf> {
    let candidate = std::env::var_os(key).map(PathBuf::from)?;
    looks_like_ctox_root(&candidate).then_some(candidate)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

/// Collect every value that follows a repeatable flag (e.g. `--to` / `--cc`).
fn collect_flag_values(args: &[String], flag: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut idx = 0;
    while idx < args.len() {
        if args[idx] == flag {
            if let Some(value) = args.get(idx + 1) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    values.push(trimmed.to_string());
                }
                idx += 2;
                continue;
            }
        }
        idx += 1;
    }
    values
}

fn chat_status_has_completed_since(completed: Option<&String>, before: Option<&String>) -> bool {
    completed.is_some_and(|completed| Some(completed) != before)
}

fn resolve_chat_attachment_paths(args: &[String]) -> anyhow::Result<Vec<String>> {
    let mut paths = Vec::new();
    let mut idx = 0usize;
    while idx < args.len() {
        if args[idx] == "--attach-file" {
            let value = args
                .get(idx + 1)
                .filter(|value| !value.trim().is_empty() && !value.starts_with("--"))
                .with_context(|| "--attach-file requires a file path")?;
            let canonical = std::fs::canonicalize(value)
                .with_context(|| format!("failed to resolve --attach-file path `{value}`"))?;
            anyhow::ensure!(
                canonical.is_file(),
                "--attach-file path is not a regular file: {}",
                canonical.display()
            );
            paths.push(canonical.to_string_lossy().to_string());
            idx += 2;
            continue;
        }
        idx += 1;
    }
    Ok(paths)
}

fn persist_runtime_turn_timeout(root: &Path, timeout: Option<&str>) -> anyhow::Result<()> {
    let mut env_map = runtime_env::load_runtime_env_map(root)?;
    let value = timeout
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("`--timeout` requires a value in seconds")?;
    let parsed = value
        .parse::<u64>()
        .context("`--timeout` must be a positive integer number of seconds")?;
    if parsed == 0 {
        anyhow::bail!("`--timeout` must be greater than 0 seconds");
    }
    env_map.insert(
        "CTOX_CHAT_TURN_TIMEOUT_SECS".to_string(),
        parsed.to_string(),
    );
    runtime_env::save_runtime_env_map(root, &env_map)?;
    Ok(())
}

fn find_ctox_root_from_ancestors(start: &Path) -> Option<PathBuf> {
    for candidate in start.ancestors() {
        if looks_like_ctox_root(candidate) {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

fn resolve_runtime_ctox_root(current_exe: &Path, home_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(root) = find_ctox_root_from_ancestors(current_exe) {
        return Some(root);
    }
    let Some(home_dir) = home_dir else {
        return None;
    };
    let default_current_root = home_dir.join(".local/lib/ctox/current");
    if looks_like_ctox_root(&default_current_root) {
        return Some(default_current_root);
    }
    resolve_systemd_user_ctox_root(home_dir)
}

fn looks_like_ctox_root(candidate: &Path) -> bool {
    let has_entrypoint =
        candidate.join("src/main.rs").is_file() || candidate.join("src/core/main.rs").is_file();
    candidate.join("Cargo.toml").is_file()
        && has_entrypoint
        && candidate
            .join("contracts/history/creation-ledger.md")
            .is_file()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn resolve_systemd_user_ctox_root(home_dir: &Path) -> Option<PathBuf> {
    let service_file = home_dir.join(".config/systemd/user/ctox.service");
    let contents = std::fs::read_to_string(service_file).ok()?;
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("Environment=CTOX_ROOT=") {
            let candidate = PathBuf::from(value.trim());
            if looks_like_ctox_root(&candidate) {
                return Some(candidate);
            }
        }
        if let Some(value) = trimmed.strip_prefix("WorkingDirectory=") {
            let candidate = PathBuf::from(value.trim());
            if looks_like_ctox_root(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn handle_mailserver_command(root: &Path, args: &[String]) -> anyhow::Result<()> {
    let subcmd = args.first().map(String::as_str).unwrap_or("");
    let db_path = paths::core_db(root).to_string_lossy().to_string();
    let store = ctox_mailserver::store::SqliteStore::new(&db_path);

    // Ensure database stalwart tables exist (in case it is the first time running)
    store
        .init()
        .context("Failed to initialize mailserver SQLite store")?;

    match subcmd {
        "add-domain" => {
            let domain = args.get(1).context("usage: ctox mailserver add-domain <domain> [--selector <selector>] [--private-key <key>]")?;
            let selector = find_flag_value(&args[2..], "--selector").unwrap_or("default");
            let private_key_arg = find_flag_value(&args[2..], "--private-key");

            let private_key_pem = if let Some(key_val) = private_key_arg {
                if key_val.starts_with("-----BEGIN") {
                    key_val.to_string()
                } else {
                    // Try to read it as a file path
                    std::fs::read_to_string(key_val).with_context(|| {
                        format!("Failed to read private key from file path: {}", key_val)
                    })?
                }
            } else {
                println!(
                    "Generiere neuen 2048-bit RSA-Schlüssel für Domain '{}'...",
                    domain
                );
                let output = std::process::Command::new("openssl")
                    .args(&["genrsa", "2048"])
                    .output()
                    .context("Failed to run 'openssl genrsa 2048'. Bitte stellen Sie sicher, dass openssl auf dem System installiert ist.")?;
                if !output.status.success() {
                    anyhow::bail!(
                        "openssl genrsa failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                String::from_utf8_lossy(&output.stdout).into_owned()
            };

            // Derive public key in DER format to generate base64 for Vercel/DNS
            let mut child = std::process::Command::new("openssl")
                .args(&["rsa", "-pubout", "-outform", "DER"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .context("Failed to spawn openssl to extract public key")?;

            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                stdin.write_all(private_key_pem.as_bytes())?;
            }

            let output = child.wait_with_output()?;
            if !output.status.success() {
                anyhow::bail!(
                    "openssl rsa public key derivation failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
            use base64::Engine;
            let b64_pubkey = BASE64_STANDARD.encode(&output.stdout);

            store.add_domain(domain, selector, &private_key_pem)?;

            println!(
                "\n\x1b[32;1m✓ Domain '{}' erfolgreich hinzugefügt.\x1b[0m",
                domain
            );
            println!(
                "\n================================================================================\n"
            );
            println!(
                "\x1b[1m=== DNS / Vercel-Konfiguration für {} ===\x1b[0m\n",
                domain
            );
            println!(
                "Für die DKIM-Signierung fügen Sie bitte folgenden TXT-Eintrag bei Ihrem DNS-Provider hinzu:\n"
            );
            println!(
                "  \x1b[33;1mName/Host:\x1b[0m   {}._domainkey.{}",
                selector, domain
            );
            println!("  \x1b[33;1mTyp:\x1b[0m         TXT");
            println!(
                "  \x1b[33;1mWert:\x1b[0m        v=DKIM1; k=rsa; p={}",
                b64_pubkey
            );
            println!("\nZusätzlich empfohlene Einträge für den E-Mail-Verkehr:\n");
            println!("  \x1b[36mMX Record:\x1b[0m");
            println!("    Name/Host:  @");
            println!("    Typ:        MX");
            println!("    Wert:       10 mail.{}", domain);
            println!("\n  \x1b[36mA Record:\x1b[0m");
            println!("    Name/Host:  mail.{}", domain);
            println!("    Typ:        A");
            println!("    Wert:       203.0.113.10");
            println!("\n  \x1b[36mTXT SPF Record:\x1b[0m");
            println!("    Name/Host:  @");
            println!("    Typ:        TXT");
            println!("    Wert:       v=spf1 mx a ip4:203.0.113.10 ~all");
            println!("\n  \x1b[36mTXT DMARC Record:\x1b[0m");
            println!("    Name/Host:  _dmarc.{}", domain);
            println!("    Typ:        TXT");
            println!(
                "    Wert:       v=DMARC1; p=quarantine; pct=100; rua=mailto:dmarc@{}",
                domain
            );
            println!(
                "\n================================================================================\n"
            );
            Ok(())
        }
        "list-domains" => {
            let conn = rusqlite::Connection::open(&db_path)?;
            let mut stmt =
                conn.prepare("SELECT domain_name, dkim_selector FROM stalwart_domains")?;
            let rows = stmt.query_map([], |row| {
                let name: String = row.get(0)?;
                let selector: String = row.get(1)?;
                Ok((name, selector))
            })?;
            println!("\n\x1b[1mRegistrierte Domains:\x1b[0m");
            println!("{:<30} {:<15}", "Domain", "Selector");
            println!("{:-<45}", "");
            let mut count = 0;
            for row in rows {
                let (name, selector) = row?;
                println!("{:<30} {:<15}", name, selector);
                count += 1;
            }
            if count == 0 {
                println!("(Keine Domains registriert)");
            }
            println!();
            Ok(())
        }
        "add-user" => {
            let email = args
                .get(1)
                .context("usage: ctox mailserver add-user <email> <password>")?;
            let password = args
                .get(2)
                .context("usage: ctox mailserver add-user <email> <password>")?;

            store.add_user(email, password)?;
            println!(
                "\n\x1b[32;1m✓ Benutzer '{}' erfolgreich erstellt.\x1b[0m",
                email
            );
            println!("Standard-Mailboxen (INBOX, Sent, Trash) wurden automatisch angelegt.\n");
            Ok(())
        }
        "list-users" => {
            let conn = rusqlite::Connection::open(&db_path)?;
            let mut stmt = conn.prepare("SELECT username, created_at FROM stalwart_users")?;
            let rows = stmt.query_map([], |row| {
                let username: String = row.get(0)?;
                let created_at: i64 = row.get(1)?;
                Ok((username, created_at))
            })?;
            println!("\n\x1b[1mRegistrierte Benutzer:\x1b[0m");
            println!("{:<40} {:<25}", "E-Mail / Benutzername", "Erstellt am");
            println!("{:-<65}", "");
            let mut count = 0;
            for row in rows {
                let (username, created_at) = row?;
                let dt = chrono::DateTime::from_timestamp(created_at, 0)
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_else(|| created_at.to_string());
                println!("{:<40} {:<25}", username, dt);
                count += 1;
            }
            if count == 0 {
                println!("(Keine Benutzer registriert)");
            }
            println!();
            Ok(())
        }
        "send-email" => {
            let from = find_flag_value(&args[1..], "--from").context("usage: ctox mailserver send-email --from <email> --to <email> --subject <subject> --body <body>\nFehlendes Argument: --from")?;
            let to = find_flag_value(&args[1..], "--to").context("usage: ctox mailserver send-email --from <email> --to <email> --subject <subject> --body <body>\nFehlendes Argument: --to")?;
            let subject = find_flag_value(&args[1..], "--subject").context("usage: ctox mailserver send-email --from <email> --to <email> --subject <subject> --body <body>\nFehlendes Argument: --subject")?;
            let body = find_flag_value(&args[1..], "--body").context("usage: ctox mailserver send-email --from <email> --to <email> --subject <subject> --body <body>\nFehlendes Argument: --body")?;

            let msg_id = format!("<{}@ctox.local>", uuid::Uuid::new_v4());
            let date = chrono::Utc::now().to_rfc2822();

            let rfc822_body = format!(
                "From: {from}\r\n\
                 To: {to}\r\n\
                 Subject: {subject}\r\n\
                 Message-ID: {msg_id}\r\n\
                 Date: {date}\r\n\
                 MIME-Version: 1.0\r\n\
                 Content-Type: text/plain; charset=utf-8\r\n\
                 Content-Transfer-Encoding: 7bit\r\n\
                 \r\n\
                 {body}\r\n",
                from = from,
                to = to,
                subject = subject,
                msg_id = msg_id,
                date = date,
                body = body
            );

            let queue_id = store.queue_email(from, to, &rfc822_body)?;
            println!("\n\x1b[32;1m✓ E-Mail erfolgreich in die Warteschlange eingereiht.\x1b[0m");
            println!("  Warteschlangen-ID: {}", queue_id);
            println!("  Absender:          {}", from);
            println!("  Empfänger:         {}", to);
            println!("  Betreff:           {}", subject);
            println!(
                "\nDer ctox-Hintergrunddienst wird diese E-Mail in Kürze automatisch versenden.\n"
            );
            Ok(())
        }
        _ => {
            anyhow::bail!(
                "Unbekannter mailserver Unterbefehl. Verfügbare Befehle:\n\n\
                 ctox mailserver add-domain <domain> [--selector <selector>] [--private-key <key>]\n\
                 ctox mailserver list-domains\n\
                 ctox mailserver add-user <email> <password>\n\
                 ctox mailserver list-users\n\
                 ctox mailserver send-email --from <email> --to <email> --subject <subject> --body <body>"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        append_appsec_credential_proof_arg, appsec_command_argv_strings,
        appsec_command_unresolved_placeholders, browser_automation_source_from_stage_command,
        build_appsec_forwarded_args, chat_status_has_completed_since,
        execute_appsec_stage_commands, find_ctox_root_from_ancestors, handle_appsec_pipeline_work,
        handle_continuity_update, looks_like_ctox_root, openrouter_tool_smoke_summary,
        persist_appsec_command_expected_artifact, persist_runtime_turn_timeout,
        record_appsec_stage_artifact_bindings, resolve_appsec_stage_command_placeholders,
        resolve_chat_attachment_paths, resolve_runtime_ctox_root, run_projected_appsec_command,
        validated_workspace_root_override, AppsecStageExecutionContext,
    };
    use crate::execution::models::runtime_env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn continuity_update_rejects_missing_conversation_id() {
        let args = [
            "--db",
            "/tmp/ctox-continuity-missing-conversation.sqlite",
            "--kind",
            "focus",
            "--mode",
            "full",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        let error = handle_continuity_update(&args)
            .expect_err("continuity update must require explicit conversation id");
        assert!(error
            .to_string()
            .contains("missing required --conversation-id"));
    }

    #[test]
    fn appsec_forwarding_uses_ctox_toolroot_for_external_state() {
        let root = unique_test_dir("appsec-external-state-toolroot");
        let external_state = root.join("external/assessment-state");
        let forwarded = build_appsec_forwarded_args(
            &root,
            &[
                "--state-dir".to_string(),
                external_state.to_string_lossy().to_string(),
                "tools".to_string(),
                "inventory".to_string(),
                "--json".to_string(),
            ],
        );

        assert_eq!(
            super::arg_value(&forwarded, "--state-dir"),
            Some(external_state.to_string_lossy().to_string())
        );
        assert_eq!(
            super::arg_value(&forwarded, "--tools-root"),
            Some(
                root.join("runtime/tools/appsec")
                    .to_string_lossy()
                    .to_string()
            )
        );

        cleanup_test_dir(&root);
    }

    #[test]
    fn finds_ctox_root_from_nested_workspace_directory() {
        let root = make_fake_ctox_root("root-ancestor");
        let nested = root.join("src/context");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(find_ctox_root_from_ancestors(&nested), Some(root.clone()));
        cleanup_test_dir(&root);
    }

    #[test]
    fn resolves_default_install_root_when_binary_runs_without_wrapper_env() {
        let home = unique_test_dir("installed-home");
        let install_root = home.join(".local/lib/ctox/current");
        create_fake_ctox_root(&install_root);
        let exe = home.join(".local/bin/ctox");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"binary").unwrap();

        assert_eq!(
            resolve_runtime_ctox_root(&exe, Some(home.as_path())),
            Some(install_root.clone())
        );

        cleanup_test_dir(&home);
    }

    #[test]
    fn resolves_systemd_service_root_when_wrapper_env_is_missing() {
        let home = unique_test_dir("systemd-home");
        let service_root = home.join("CTOX");
        create_fake_ctox_root(&service_root);
        let systemd_dir = home.join(".config/systemd/user");
        fs::create_dir_all(&systemd_dir).unwrap();
        fs::write(
            systemd_dir.join("ctox.service"),
            format!(
                "[Service]\nWorkingDirectory={}\nEnvironment=CTOX_ROOT={}\n",
                service_root.display(),
                service_root.display()
            ),
        )
        .unwrap();
        let exe = home.join(".local/bin/ctox");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"binary").unwrap();

        assert_eq!(
            resolve_runtime_ctox_root(&exe, Some(home.as_path())),
            Some(service_root.clone())
        );

        cleanup_test_dir(&home);
    }

    #[test]
    fn rejects_directories_without_ctox_markers() {
        let root = unique_test_dir("not-root");
        fs::create_dir_all(&root).unwrap();

        assert!(!looks_like_ctox_root(&root));

        cleanup_test_dir(&root);
    }

    #[test]
    fn workspace_root_override_accepts_valid_ctox_root_only() {
        let root = make_fake_ctox_root("env-valid-root");
        let invalid = unique_test_dir("env-invalid-root");
        fs::create_dir_all(&invalid).unwrap();

        std::env::set_var("CTOX_HOME", &root);
        assert_eq!(
            validated_workspace_root_override("CTOX_HOME"),
            Some(root.clone())
        );

        std::env::set_var("CTOX_HOME", &invalid);
        assert_eq!(validated_workspace_root_override("CTOX_HOME"), None);

        std::env::remove_var("CTOX_HOME");
        cleanup_test_dir(&root);
        cleanup_test_dir(&invalid);
    }

    #[test]
    fn persist_runtime_turn_timeout_writes_to_runtime_env_store() {
        let root = make_fake_ctox_root("runtime-timeout");

        persist_runtime_turn_timeout(&root, Some("900")).unwrap();

        let env_map = runtime_env::load_runtime_env_map(&root).unwrap();
        assert_eq!(
            env_map
                .get("CTOX_CHAT_TURN_TIMEOUT_SECS")
                .map(String::as_str),
            Some("900")
        );

        cleanup_test_dir(&root);
    }

    #[test]
    fn persist_runtime_turn_timeout_rejects_zero_seconds() {
        let root = make_fake_ctox_root("runtime-timeout-zero");

        let err = persist_runtime_turn_timeout(&root, Some("0")).unwrap_err();
        assert!(err
            .to_string()
            .contains("`--timeout` must be greater than 0 seconds"));

        cleanup_test_dir(&root);
    }

    #[test]
    fn persist_runtime_turn_timeout_requires_numeric_value() {
        let root = make_fake_ctox_root("runtime-timeout-invalid");

        let err = persist_runtime_turn_timeout(&root, Some("fast")).unwrap_err();
        assert!(err
            .to_string()
            .contains("`--timeout` must be a positive integer number of seconds"));

        cleanup_test_dir(&root);
    }

    #[test]
    fn appsec_authz_preflight_injects_redacted_credential_proof() {
        let root = make_fake_ctox_root("appsec-credential-proof");
        let state = root.join("runtime/appsec/proof-test");
        let authz_dir = state.join("authz");
        fs::create_dir_all(&authz_dir).unwrap();
        let subjects = authz_dir.join("authz-subjects.json");
        fs::write(
            &subjects,
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": "ctox.appsec_pentest.authz_subjects.v1",
                "subjects": [
                    {"id": "user-a", "role": "owner", "login_hint": "a@example.test", "credential_ref": "ctox-secret://appsec/a", "verify_selector": "[data-testid='account-shell']"},
                    {"id": "user-b", "role": "member", "login_hint": "b@example.test", "credential_ref": "ctox-secret://appsec/b", "verify_selector": "[data-testid='account-shell']"}
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        crate::secrets::write_secret_record(
            &root,
            "appsec",
            "a",
            "super-sensitive-password",
            Some("test authz credential".to_string()),
            serde_json::json!({"source": "test"}),
        )
        .unwrap();

        let args = vec![
            "--state-dir".to_string(),
            state.to_string_lossy().to_string(),
            "--tools-root".to_string(),
            root.join("runtime/tools/appsec")
                .to_string_lossy()
                .to_string(),
            "authz".to_string(),
            "preflight".to_string(),
            "--target".to_string(),
            "https://example.test/app".to_string(),
            "--subjects".to_string(),
            subjects.to_string_lossy().to_string(),
            "--require-credentials".to_string(),
        ];
        let augmented = append_appsec_credential_proof_arg(&root, &args).unwrap();
        let proof_arg = augmented
            .windows(2)
            .find(|window| window.first().map(String::as_str) == Some("--credential-proof"))
            .and_then(|window| window.get(1))
            .map(PathBuf::from)
            .expect("credential proof arg");
        let proof_text = fs::read_to_string(&proof_arg).unwrap();
        assert!(!proof_text.contains("super-sensitive-password"));
        let proof: serde_json::Value = serde_json::from_str(&proof_text).unwrap();
        assert_eq!(
            proof.get("version").and_then(serde_json::Value::as_str),
            Some("ctox.appsec_pentest.authz_credential_proof.v1")
        );
        assert_eq!(
            proof
                .pointer("/summary/available")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            proof
                .pointer("/summary/missing")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert!(proof
            .get("subjects")
            .and_then(serde_json::Value::as_array)
            .unwrap()
            .iter()
            .any(|row| {
                row.get("subject_id").and_then(serde_json::Value::as_str) == Some("user-b")
                    && row.get("status").and_then(serde_json::Value::as_str) == Some("missing")
            }));

        cleanup_test_dir(&root);
    }

    #[test]
    fn chat_attachment_paths_are_canonicalized_for_service_send() {
        let root = unique_test_dir("chat-attachments");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("report.xlsx");
        fs::write(&file, b"xlsx").unwrap();

        let args = vec![
            "Mail".to_string(),
            "--attach-file".to_string(),
            file.to_string_lossy().to_string(),
        ];

        let attachments = resolve_chat_attachment_paths(&args).unwrap();
        assert_eq!(
            attachments,
            vec![fs::canonicalize(&file)
                .unwrap()
                .to_string_lossy()
                .to_string()]
        );

        cleanup_test_dir(&root);
    }

    #[test]
    fn chat_attachment_paths_require_existing_files() {
        let args = vec![
            "Mail".to_string(),
            "--attach-file".to_string(),
            "/definitely/missing/report.xlsx".to_string(),
        ];

        let err = resolve_chat_attachment_paths(&args).unwrap_err();
        assert!(err
            .to_string()
            .contains("failed to resolve --attach-file path"));
    }

    #[test]
    fn chat_wait_completion_ignores_unrelated_pending_queue() {
        let before = Some("2026-05-06T23:50:00Z".to_string());
        let completed = Some("2026-05-06T23:51:00Z".to_string());

        assert!(chat_status_has_completed_since(
            completed.as_ref(),
            before.as_ref()
        ));
        assert!(!chat_status_has_completed_since(
            before.as_ref(),
            before.as_ref()
        ));
    }

    #[test]
    fn openrouter_tool_smoke_summary_detects_tool_call_response() {
        let payload = serde_json::json!({
            "model": "deepseek/deepseek-v4-flash-20260423",
            "provider": "DeepInfra",
            "choices": [{
                "finish_reason": "tool_calls",
                "native_finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "record_status",
                            "arguments": "{\"status\":\"ok\"}"
                        }
                    }]
                }
            }]
        });

        let summary = openrouter_tool_smoke_summary("auto", 200, &payload);
        assert_eq!(summary["has_tool_calls"], serde_json::json!(true));
        assert_eq!(summary["tool_call_count"], serde_json::json!(1));
        assert_eq!(
            summary["tool_call_names"],
            serde_json::json!(["record_status"])
        );
        assert_eq!(summary["content_len"], serde_json::json!(0));
    }

    #[cfg(unix)]
    #[test]
    fn appsec_pipeline_worker_executes_ready_stage_and_handles_queue() {
        use std::os::unix::fs::PermissionsExt;

        let root = make_fake_ctox_root("appsec-pipeline-worker");
        let state = root.join("runtime/appsec/default");
        fs::create_dir_all(&state).unwrap();
        let bin_dir = root.join("runtime/tools/appsec/bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let httpx = bin_dir.join("httpx");
        fs::write(
            &httpx,
            "#!/bin/sh\nprintf '{\"url\":\"https://example.test\",\"status_code\":200}\\n'\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&httpx).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&httpx, permissions).unwrap();

        run_projected_appsec_command(
            &root,
            &[
                "--state-dir".to_string(),
                state.to_string_lossy().to_string(),
                "init".to_string(),
                "--url".to_string(),
                "https://example.test".to_string(),
            ],
        )
        .unwrap();
        fs::write(
            state.join("assessment-pipeline.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "ctox.appsec_pentest.assessment_pipeline.v1",
                "generated_at": "test",
                "profile": "minimal",
                "active": false,
                "stages": [{
                    "id": "stage-1-blackbox-map",
                    "order": 1,
                    "phase": "blackbox-map",
                    "target": "https://example.test",
                    "tools": ["httpx"],
                    "active_required": false,
                    "readiness_blockers": [],
                    "completion_gate": "httpx mapping run artifact"
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            state.join("coverage.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "ctox.appsec_pentest.coverage.v1",
                "workstreams": [{
                    "id": "ws-map",
                    "phase": "blackbox-map",
                    "target": "https://example.test",
                    "status": "planned",
                    "tools": ["httpx"]
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        let enqueue = run_projected_appsec_command(
            &root,
            &[
                "pipeline".to_string(),
                "enqueue".to_string(),
                "--state-dir".to_string(),
                state.to_string_lossy().to_string(),
                "--workspace-root".to_string(),
                root.to_string_lossy().to_string(),
            ],
        )
        .unwrap();
        assert_eq!(
            enqueue
                .pointer("/ctox_queue_enqueue/created_or_updated")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );

        let worker = handle_appsec_pipeline_work(
            &root,
            &[
                "pipeline".to_string(),
                "work".to_string(),
                "--state-dir".to_string(),
                state.to_string_lossy().to_string(),
                "--limit".to_string(),
                "1".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(
            worker
                .pointer("/summary/handled")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        let message_key = worker
            .pointer("/tasks/0/message_key")
            .and_then(serde_json::Value::as_str)
            .unwrap();
        let task = crate::channels::load_queue_task(&root, message_key)
            .unwrap()
            .unwrap();
        assert_eq!(task.route_status, "handled");

        let coverage: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(state.join("coverage.json")).unwrap())
                .unwrap();
        assert_eq!(
            coverage
                .pointer("/workstreams/0/status")
                .and_then(serde_json::Value::as_str),
            Some("completed")
        );
        let writeback: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(state.join("assessment-pipeline-writeback.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            writeback
                .pointer("/stages/0/status")
                .and_then(serde_json::Value::as_str),
            Some("completed")
        );
        let conn = rusqlite::Connection::open(crate::paths::core_db(&root)).unwrap();
        let proof_count: i64 = conn
            .query_row(
                "SELECT COUNT(*)
                 FROM ctox_core_transition_proofs
                 WHERE entity_type = 'QueueItem'
                   AND entity_id = ?1
                   AND to_state = 'Completed'
                   AND accepted = 1
                   AND request_json LIKE '%appsec-pipeline-stage-terminal-success%'",
                [message_key],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(proof_count, 1);

        cleanup_test_dir(&root);
    }

    #[test]
    fn appsec_pipeline_worker_completes_authz_stage_from_redacted_web_stack_evidence() {
        let root = make_fake_ctox_root("appsec-pipeline-authz-e2e");
        let state = root.join("runtime/appsec/default");
        let authz_dir = state.join("authz");
        fs::create_dir_all(&authz_dir).unwrap();
        let target = "https://example.test/app";
        let write_json_file = |path: PathBuf, value: serde_json::Value| {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
        };

        run_projected_appsec_command(
            &root,
            &[
                "--state-dir".to_string(),
                state.to_string_lossy().to_string(),
                "init".to_string(),
                "--url".to_string(),
                target.to_string(),
            ],
        )
        .unwrap();
        write_json_file(
            authz_dir.join("authz-subjects.json"),
            serde_json::json!({
                "version": "ctox.appsec_pentest.authz_subjects.v1",
                "subjects": [
                    {
                        "id": "user-a",
                        "role": "customer",
                        "login_hint": "user-a@example.test",
                        "credential_ref": "ctox-secret://appsec/user-a",
                        "verify_selector": "[data-testid='account-shell']"
                    },
                    {
                        "id": "user-b",
                        "role": "customer",
                        "login_hint": "user-b@example.test",
                        "credential_ref": "ctox-secret://appsec/user-b",
                        "verify_selector": "[data-testid='account-shell']"
                    }
                ]
            }),
        );
        for (subject, object_ref) in [("user-a", "tenant-a"), ("user-b", "tenant-b")] {
            write_json_file(
                authz_dir.join(subject).join("auth-assist-redacted.json"),
                serde_json::json!({
                    "version": "ctox.appsec_pentest.web_stack_evidence.v1",
                    "source_tool": "ctox_web_auth_assist_request",
                    "phase": "subject-auth",
                    "subject_id": subject,
                    "ok": true,
                    "status": "pending_sync",
                    "session_id": format!("browser_session_{subject}"),
                    "redacted": true
                }),
            );
            write_json_file(
                authz_dir.join(subject).join("auth-login-redacted.json"),
                serde_json::json!({
                    "version": "ctox.appsec_pentest.web_stack_evidence.v1",
                    "source_tool": "ctox_web_auth_assist_login",
                    "phase": "subject-auth-login",
                    "subject_id": subject,
                    "ok": true,
                    "status": "completed",
                    "login_state": "authenticated",
                    "mfa_required": false,
                    "login_error_detected": false,
                    "verify_selector_found": true,
                    "session_id": format!("browser_session_{subject}"),
                    "redacted": true
                }),
            );
            write_json_file(
                authz_dir.join(subject).join("same-origin-api-map.json"),
                serde_json::json!({
                    "version": "ctox.appsec_pentest.web_stack_evidence.v1",
                    "source_tool": "ctox_browser_automation",
                    "phase": "subject-crawl",
                    "subject_id": subject,
                    "ok": true,
                    "status": "completed",
                    "result": {
                        "source": "ctox_browser_automation",
                        "task_type": "same-origin-api-map",
                        "subject_id": subject,
                        "objects": [{
                            "object_ref": object_ref,
                            "object_type": "tenant",
                            "owner_subject": subject
                        }],
                        "replay_candidates": [{
                            "endpoint": format!("/api/instances/{object_ref}/health"),
                            "method": "GET",
                            "status": 200,
                            "resource_type": "fetch",
                            "body_class": "tenant-json",
                            "owner_body_hash": format!("{object_ref}-owner-body-hash"),
                            "owner_body_length": 128,
                            "owner_object_refs": [object_ref],
                            "object": object_ref,
                            "object_type": "tenant",
                            "object_source": "url",
                            "owner_subject": subject,
                            "expected": "deny"
                        }]
                    },
                    "redacted": true
                }),
            );
            write_json_file(
                authz_dir
                    .join(subject)
                    .join("browser-context-reference.json"),
                serde_json::json!({
                    "version": "ctox.appsec_pentest.web_stack_evidence.v1",
                    "source_tool": "ctox_browser_context_capture",
                    "phase": "subject-context-capture",
                    "subject_id": subject,
                    "ok": true,
                    "status": "pending_sync",
                    "result": {
                        "ok": true,
                        "status": "pending_sync",
                        "browser_context_artifact": "redacted-reference"
                    },
                    "redacted": true
                }),
            );
            write_json_file(
                authz_dir
                    .join(subject)
                    .join("browser-context-extract-redacted.json"),
                serde_json::json!({
                    "version": "ctox.appsec_pentest.web_stack_evidence.v1",
                    "source_tool": "ctox_browser_context_extract",
                    "phase": "subject-context-extract",
                    "subject_id": subject,
                    "ok": true,
                    "status": "pending_sync",
                    "result": {
                        "ok": true,
                        "status": "pending_sync",
                        "browser_context_artifact": "redacted-reference"
                    },
                    "redacted": true
                }),
            );
        }
        for (owner, actor, object_ref, actual_status, result, leak) in [
            ("user-a", "user-b", "tenant-a", 200, "fail", true),
            ("user-b", "user-a", "tenant-b", 404, "pass", false),
        ] {
            write_json_file(
                authz_dir
                    .join("replay")
                    .join(format!("{owner}-as-{actor}.json")),
                serde_json::json!({
                    "version": "ctox.appsec_pentest.web_stack_evidence.v1",
                    "source_tool": "ctox_browser_automation",
                    "phase": "cross-subject-replay",
                    "owner_subject": owner,
                    "actor_subject": actor,
                    "ok": true,
                    "status": "completed",
                    "result": {
                        "source": "ctox_browser_automation",
                        "task_type": "cross-subject-replay",
                        "owner_subject": owner,
                        "actor_subject": actor,
                        "objects": [{
                            "object_ref": object_ref,
                            "object_type": "tenant",
                            "owner_subject": owner
                        }],
                        "cases": [{
                            "actor_subject": actor,
                            "owner_subject": owner,
                            "object_ref": object_ref,
                            "object_type": "tenant",
                            "endpoint": format!("/api/instances/{object_ref}/health"),
                            "method": "GET",
                            "expected": "deny",
                            "actual_status": actual_status,
                            "result": result,
                            "body_class": if leak { "tenant-json" } else { "not-found" },
                            "leak": leak,
                            "mutation": false,
                            "evidence_artifact": format!("authz/replay/{owner}-as-{actor}.json")
                        }]
                    },
                    "redacted": true
                }),
            );
        }
        write_json_file(
            state.join("assessment-pipeline.json"),
            serde_json::json!({
                "version": "ctox.appsec_pentest.assessment_pipeline.v1",
                "generated_at": "test",
                "profile": "standard",
                "active": false,
                "stages": [{
                    "id": "stage-1-authenticated-multi-user-authz",
                    "order": 1,
                    "phase": "authenticated-multi-user-authz",
                    "target": target,
                    "tools": ["ctox-web-stack", "ctox_browser_automation"],
                    "active_required": false,
                    "readiness_blockers": [],
                    "completion_gate": "imported authz matrix with redacted browser evidence"
                }]
            }),
        );
        write_json_file(
            state.join("coverage.json"),
            serde_json::json!({
                "version": "ctox.appsec_pentest.coverage.v1",
                "workstreams": [{
                    "id": "ws-authz",
                    "phase": "authenticated-multi-user-authz",
                    "target": target,
                    "status": "planned",
                    "tools": ["ctox-web-stack", "ctox_browser_automation"]
                }]
            }),
        );

        let enqueue = run_projected_appsec_command(
            &root,
            &[
                "pipeline".to_string(),
                "enqueue".to_string(),
                "--state-dir".to_string(),
                state.to_string_lossy().to_string(),
                "--workspace-root".to_string(),
                root.to_string_lossy().to_string(),
            ],
        )
        .unwrap();
        assert_eq!(
            enqueue
                .pointer("/ctox_queue_enqueue/created_or_updated")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );

        let worker = handle_appsec_pipeline_work(
            &root,
            &[
                "pipeline".to_string(),
                "work".to_string(),
                "--state-dir".to_string(),
                state.to_string_lossy().to_string(),
                "--limit".to_string(),
                "1".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(
            worker
                .pointer("/summary/handled")
                .and_then(serde_json::Value::as_u64),
            Some(1),
            "{worker:#}"
        );
        assert!(worker
            .pointer("/tasks/0/execution/commands")
            .and_then(serde_json::Value::as_array)
            .unwrap()
            .iter()
            .any(|command| {
                command
                    .pointer("/output/reused_existing_artifact")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                    && command
                        .pointer("/output/result/source_tool")
                        .and_then(serde_json::Value::as_str)
                        == Some("ctox_web_auth_assist_login")
            }));
        assert_eq!(
            worker
                .pointer("/tasks/0/execution/artifact_bindings/authz-run")
                .and_then(serde_json::Value::as_str)
                .map(|value| value.contains("authz-run-")),
            Some(true)
        );
        let coverage: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(state.join("coverage.json")).unwrap())
                .unwrap();
        assert_eq!(
            coverage
                .pointer("/workstreams/0/status")
                .and_then(serde_json::Value::as_str),
            Some("completed")
        );
        let findings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(state.join("findings.json")).unwrap())
                .unwrap();
        assert!(findings.as_array().unwrap().iter().any(|finding| {
            finding
                .get("source_tool")
                .and_then(serde_json::Value::as_str)
                == Some("ctox-web-stack-authz")
                && finding.get("category").and_then(serde_json::Value::as_str) == Some("idor")
        }));
        let message_key = worker
            .pointer("/tasks/0/message_key")
            .and_then(serde_json::Value::as_str)
            .unwrap();
        let task = crate::channels::load_queue_task(&root, message_key)
            .unwrap()
            .unwrap();
        assert_eq!(task.route_status, "handled");

        cleanup_test_dir(&root);
    }

    #[cfg(unix)]
    #[test]
    fn appsec_pipeline_worker_retries_transient_tool_failure_until_budget_exhausted() {
        use std::os::unix::fs::PermissionsExt;

        let root = make_fake_ctox_root("appsec-pipeline-retry-budget");
        let state = root.join("runtime/appsec/default");
        fs::create_dir_all(&state).unwrap();
        let bin_dir = root.join("runtime/tools/appsec/bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let httpx = bin_dir.join("httpx");
        fs::write(
            &httpx,
            "#!/bin/sh\nprintf 'transient failure\\n' >&2\nexit 2\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&httpx).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&httpx, permissions).unwrap();

        run_projected_appsec_command(
            &root,
            &[
                "--state-dir".to_string(),
                state.to_string_lossy().to_string(),
                "init".to_string(),
                "--url".to_string(),
                "https://example.test".to_string(),
            ],
        )
        .unwrap();
        fs::write(
            state.join("assessment-pipeline.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "ctox.appsec_pentest.assessment_pipeline.v1",
                "generated_at": "test",
                "profile": "minimal",
                "active": false,
                "stages": [{
                    "id": "stage-1-blackbox-map",
                    "order": 1,
                    "phase": "blackbox-map",
                    "target": "https://example.test",
                    "tools": ["httpx"],
                    "active_required": false,
                    "readiness_blockers": [],
                    "completion_gate": "httpx mapping run artifact"
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            state.join("coverage.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "ctox.appsec_pentest.coverage.v1",
                "workstreams": [{
                    "id": "ws-map",
                    "phase": "blackbox-map",
                    "target": "https://example.test",
                    "status": "planned",
                    "tools": ["httpx"]
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        let enqueue = run_projected_appsec_command(
            &root,
            &[
                "pipeline".to_string(),
                "enqueue".to_string(),
                "--state-dir".to_string(),
                state.to_string_lossy().to_string(),
                "--workspace-root".to_string(),
                root.to_string_lossy().to_string(),
            ],
        )
        .unwrap();
        assert_eq!(
            enqueue
                .pointer("/ctox_queue_enqueue/created_or_updated")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );

        for expected_attempt in 1..=3 {
            let worker = handle_appsec_pipeline_work(
                &root,
                &[
                    "pipeline".to_string(),
                    "work".to_string(),
                    "--state-dir".to_string(),
                    state.to_string_lossy().to_string(),
                    "--limit".to_string(),
                    "1".to_string(),
                ],
            )
            .unwrap();
            let message_key = worker
                .pointer("/tasks/0/message_key")
                .and_then(serde_json::Value::as_str)
                .unwrap()
                .to_string();
            let retry =
                crate::channels::queue_task_metadata_value(&root, &message_key, "appsec_retry")
                    .unwrap()
                    .unwrap();
            assert_eq!(
                retry
                    .pointer("/failed_attempts")
                    .and_then(serde_json::Value::as_u64),
                Some(expected_attempt)
            );

            if expected_attempt < 3 {
                assert_eq!(
                    worker
                        .pointer("/summary/retry_scheduled")
                        .and_then(serde_json::Value::as_u64),
                    Some(1)
                );
                let task = crate::channels::load_queue_task(&root, &message_key)
                    .unwrap()
                    .unwrap();
                assert_eq!(task.route_status, "pending");

                let held = handle_appsec_pipeline_work(
                    &root,
                    &[
                        "pipeline".to_string(),
                        "work".to_string(),
                        "--state-dir".to_string(),
                        state.to_string_lossy().to_string(),
                        "--limit".to_string(),
                        "1".to_string(),
                    ],
                )
                .unwrap();
                assert_eq!(
                    held.pointer("/summary/selected")
                        .and_then(serde_json::Value::as_u64),
                    Some(0),
                    "retry not_before must stop a hot retry loop"
                );

                let mut due_retry = retry;
                due_retry["not_before"] =
                    serde_json::Value::String("1970-01-01T00:00:00Z".to_string());
                crate::channels::set_queue_task_metadata_value(
                    &root,
                    &message_key,
                    "appsec_retry",
                    due_retry,
                )
                .unwrap();
            } else {
                assert_eq!(
                    worker
                        .pointer("/summary/failed")
                        .and_then(serde_json::Value::as_u64),
                    Some(1)
                );
                let task = crate::channels::load_queue_task(&root, &message_key)
                    .unwrap()
                    .unwrap();
                assert_eq!(task.route_status, "failed");
            }
        }

        cleanup_test_dir(&root);
    }

    #[test]
    fn appsec_pipeline_worker_blocks_non_retryable_session_placeholder() {
        let root = make_fake_ctox_root("appsec-pipeline-nonretryable-blocker");
        let state = root.join("runtime/appsec/default");
        fs::create_dir_all(&state).unwrap();
        let stage = serde_json::json!({
            "id": "stage-authz-replay",
            "phase": "authenticated-multi-user-authz",
            "target": "https://example.test",
            "run_commands": [{
                "kind": "ctox-cli",
                "tool": "ctox_browser_automation",
                "ctox_cli": {
                    "program": "ctox",
                    "args": [
                        "web",
                        "browser-automation",
                        "--session-id",
                        "${session_id:user-a}",
                        "--source",
                        "return { ok: true };"
                    ]
                }
            }]
        });
        fs::write(
            state.join("assessment-pipeline.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "ctox.appsec_pentest.assessment_pipeline.v1",
                "generated_at": "test",
                "profile": "standard",
                "active": false,
                "stages": [stage.clone()]
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            state.join("coverage.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "ctox.appsec_pentest.coverage.v1",
                "workstreams": [{
                    "id": "ws-authz",
                    "phase": "authenticated-multi-user-authz",
                    "target": "https://example.test",
                    "status": "planned",
                    "tools": []
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        let task = crate::channels::create_queue_task(
            &root,
            crate::channels::QueueTaskCreateRequest {
                title: "Deployment audit stage: authz replay".to_string(),
                prompt: "Run replay with an existing session.".to_string(),
                thread_key: "appsec:authz:https-example-test".to_string(),
                workspace_root: Some(root.to_string_lossy().to_string()),
                priority: "normal".to_string(),
                suggested_skill: Some("appsec-pentest".to_string()),
                parent_message_key: None,
                extra_metadata: Some(serde_json::json!({
                    "source": "ctox-appsec-pipeline",
                    "idempotency_key": "appsec-stage-authz-replay",
                    "appsec_state_dir": state.to_string_lossy(),
                    "stage": stage,
                })),
            },
        )
        .unwrap();

        let worker = handle_appsec_pipeline_work(
            &root,
            &[
                "pipeline".to_string(),
                "work".to_string(),
                "--state-dir".to_string(),
                state.to_string_lossy().to_string(),
                "--message-key".to_string(),
                task.message_key.clone(),
            ],
        )
        .unwrap();
        assert_eq!(
            worker
                .pointer("/summary/blocked")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            worker
                .pointer("/tasks/0/failure_policy/retryable")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        let task = crate::channels::load_queue_task(&root, &task.message_key)
            .unwrap()
            .unwrap();
        assert_eq!(task.route_status, "blocked");

        cleanup_test_dir(&root);
    }

    #[test]
    fn appsec_worker_stops_authz_web_stack_chain_after_failed_gate_command() {
        let root = make_fake_ctox_root("appsec-pipeline-authz-stop-on-failure");
        let state = root.join("runtime/appsec/default");
        fs::create_dir_all(&state).unwrap();
        run_projected_appsec_command(
            &root,
            &[
                "--state-dir".to_string(),
                state.to_string_lossy().to_string(),
                "init".to_string(),
                "--url".to_string(),
                "https://example.test/app".to_string(),
            ],
        )
        .unwrap();
        let stage = serde_json::json!({
            "run_commands": [
                {
                    "kind": "ctox-cli",
                    "tool": "ctox_appsec_authz_run",
                    "stop_on_failure": true,
                    "ctox_cli": {
                        "program": "ctox",
                        "args": [
                            "appsec",
                            "authz",
                            "run",
                            "--state-dir",
                            state.to_string_lossy(),
                            "--target",
                            "https://example.test/app"
                        ]
                    }
                },
                {
                    "kind": "ctox-cli",
                    "tool": "ctox_browser_automation",
                    "ctox_cli": {
                        "program": "ctox",
                        "args": ["web", "browser-automation", "--timeout-ms", "1000"],
                        "stdin": "harness_tool.freeform_source"
                    },
                    "harness_tool": {
                        "kind": "freeform",
                        "name": "ctox_browser_automation",
                        "freeform_source": "return { ok: true, should_not_run: true };"
                    }
                }
            ]
        });

        let execution = execute_appsec_stage_commands(&root, &state, &stage).unwrap();
        let commands = execution
            .get("commands")
            .and_then(serde_json::Value::as_array)
            .unwrap();
        assert_eq!(commands.len(), 1, "{execution:#}");
        assert_eq!(
            commands[0].get("ok").and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            commands[0]
                .pointer("/output/command")
                .and_then(serde_json::Value::as_str),
            Some("authz run")
        );

        cleanup_test_dir(&root);
    }

    #[test]
    fn appsec_worker_dispatches_business_os_web_stack_auth_assist_contract() {
        let root = make_fake_ctox_root("appsec-web-stack-auth-assist");
        let state = root.join("runtime/appsec/default");
        fs::create_dir_all(&state).unwrap();
        let stage = serde_json::json!({
            "run_commands": [{
                "kind": "ctox-cli",
                "tool": "ctox_web_auth_assist_request",
                "ctox_cli": {
                    "program": "ctox",
                    "args": [
                        "business-os",
                        "web-stack",
                        "auth-assist-request",
                        "--source-id",
                        "custom-web-app",
                        "--target-url",
                        "https://example.test/login",
                        "--credential-ref",
                        "ctox-secret://appsec/user-a",
                        "--login-hint",
                        "user-a@example.test",
                        "--task-id",
                        "authz-user-a-auth-assist"
                    ]
                },
                "produces": {
                    "session_id": {
                        "placeholder": "${session_id:user-a}"
                    }
                }
            }]
        });

        let execution = execute_appsec_stage_commands(&root, &state, &stage).unwrap();
        assert_eq!(
            execution
                .pointer("/commands/0/ok")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            execution
                .pointer("/commands/0/status")
                .and_then(serde_json::Value::as_str),
            Some("pending_sync")
        );
        assert_eq!(
            execution
                .pointer("/commands/0/output/secret_value_in_payload")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            execution
                .pointer("/commands/0/output/credential_ref")
                .and_then(serde_json::Value::as_str),
            Some("ctox-secret://appsec/user-a")
        );
        assert_eq!(
            execution
                .pointer("/commands/0/output/login_hint")
                .and_then(serde_json::Value::as_str),
            Some("user-a@example.test")
        );
        assert_eq!(
            execution
                .pointer("/commands/0/output/source_id")
                .and_then(serde_json::Value::as_str),
            Some("custom-web-app")
        );
        assert!(execution
            .pointer("/commands/0/output/session_id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value.contains("authz_user_a_auth_assist")));
        let session_id = execution
            .pointer("/commands/0/output/session_id")
            .and_then(serde_json::Value::as_str)
            .unwrap();
        assert_eq!(
            execution
                .pointer("/commands/0/session_bindings/0/binding")
                .and_then(serde_json::Value::as_str),
            Some("user-a")
        );
        assert_eq!(
            execution
                .pointer("/session_bindings/user-a")
                .and_then(serde_json::Value::as_str),
            Some(session_id)
        );
        assert!(execution
            .pointer("/commands/0/output/command_id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.is_empty()));

        cleanup_test_dir(&root);
    }

    #[test]
    fn appsec_worker_resolves_web_stack_session_placeholders_before_dispatch() {
        let mut context = AppsecStageExecutionContext::default();
        context.session_ids.insert(
            "user-a".to_string(),
            "browser_session_web_stack_auth_custom_user_a".to_string(),
        );
        let command = serde_json::json!({
            "kind": "ctox-cli",
            "tool": "ctox_browser_context_capture",
            "harness_tool": {
                "kind": "freeform",
                "name": "ctox_browser_automation",
                "freeform_source": "const sessionIdRef = \"${session_id:user-a}\"; return { sessionIdRef };"
            },
            "ctox_cli": {
                "program": "ctox",
                "args": [
                    "business-os",
                    "web-stack",
                    "context-capture",
                    "--session-id",
                    "${session_id:user-a}",
                    "--source-id",
                    "custom-web-app"
                ],
                "stdin": "harness_tool.freeform_source"
            }
        });

        let (resolved, proof) =
            resolve_appsec_stage_command_placeholders(command, &context).unwrap();
        let argv = appsec_command_argv_strings(&resolved).unwrap();
        assert!(argv
            .iter()
            .any(|arg| { arg == "browser_session_web_stack_auth_custom_user_a" }));
        assert!(resolved
            .pointer("/harness_tool/freeform_source")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|source| source.contains("browser_session_web_stack_auth_custom_user_a")));
        assert!(appsec_command_unresolved_placeholders(&resolved, &argv).is_empty());
        assert_eq!(proof.len(), 2);
        assert_eq!(
            proof
                .iter()
                .filter(|item| {
                    item.get("binding").and_then(serde_json::Value::as_str) == Some("user-a")
                })
                .count(),
            2
        );
    }

    #[test]
    fn appsec_worker_resolves_authz_artifact_placeholders_before_dispatch() {
        let mut context = AppsecStageExecutionContext::default();
        context.artifacts.insert(
            "authz-run".to_string(),
            "/tmp/ctox/authz/authz-run-1.json".to_string(),
        );
        let command = serde_json::json!({
            "kind": "ctox-cli",
            "tool": "ctox_appsec_authz_build_matrix",
            "ctox_cli": {
                "program": "ctox",
                "args": [
                    "appsec",
                    "authz",
                    "build-matrix",
                    "--run",
                    "${artifact:authz-run}",
                    "--evidence-dir",
                    "/tmp/ctox/authz",
                    "--import"
                ]
            }
        });

        let (resolved, proof) =
            resolve_appsec_stage_command_placeholders(command, &context).unwrap();
        let argv = appsec_command_argv_strings(&resolved).unwrap();
        assert!(argv
            .iter()
            .any(|arg| arg == "/tmp/ctox/authz/authz-run-1.json"));
        assert!(appsec_command_unresolved_placeholders(&resolved, &argv).is_empty());
        assert_eq!(
            proof
                .iter()
                .filter(|item| {
                    item.get("binding").and_then(serde_json::Value::as_str) == Some("authz-run")
                })
                .count(),
            1
        );
    }

    #[test]
    fn appsec_worker_binds_authz_run_placeholder_to_real_run_artifact() {
        let mut context = AppsecStageExecutionContext::default();
        let command = serde_json::json!({
            "kind": "ctox-cli",
            "tool": "ctox_appsec_authz_run",
            "produces": {
                "artifact": {
                    "placeholder": "${artifact:authz-run}",
                    "json_path": "/artifact"
                }
            },
            "expected_artifact": "authz/authz-run-redacted.json"
        });
        let output = serde_json::json!({
            "ok": true,
            "artifact": "/tmp/ctox/.pentest/authz/authz-run-real.json"
        });

        let bindings = record_appsec_stage_artifact_bindings(
            &mut context,
            &command,
            &output,
            Some("/tmp/ctox/.pentest/authz/authz-run-redacted.json"),
        );

        assert_eq!(
            context.artifacts.get("authz-run").map(String::as_str),
            Some("/tmp/ctox/.pentest/authz/authz-run-real.json")
        );
        assert_eq!(
            bindings
                .first()
                .and_then(|binding| binding.get("artifact"))
                .and_then(serde_json::Value::as_str),
            Some("/tmp/ctox/.pentest/authz/authz-run-real.json")
        );
    }

    #[test]
    fn appsec_replay_source_injects_candidates_from_owner_crawl_artifact() {
        let root = make_fake_ctox_root("appsec-authz-replay-candidates");
        let state = root.join("runtime/appsec/default");
        fs::create_dir_all(state.join("authz/user-a")).unwrap();
        fs::write(
            state.join("authz/user-a/same-origin-api-map.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "ctox.appsec_pentest.web_stack_evidence.v1",
                "result": {
                    "api_requests": [{
                        "endpoint": "/assets/app.8f3c1a2b.js",
                        "method": "GET",
                        "status": 200
                    }],
                    "visible_links": [{
                        "endpoint": "/pricing",
                        "method": "GET"
                    }],
                    "replay_candidates": [{
                        "endpoint": "/api/projects/123",
                        "method": "GET",
                        "expected": "deny",
                        "object": "123",
                        "object_source": "url",
                        "owner_body_hash": "abc123",
                        "owner_object_refs": ["123"]
                    }, {
                        "endpoint": "/api/projects",
                        "method": "GET",
                        "expected": "inconclusive"
                    }, {
                        "endpoint": "/assets/project-123.css",
                        "method": "GET",
                        "expected": "deny",
                        "object": "123"
                    }]
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let command = serde_json::json!({
            "kind": "ctox-cli",
            "tool": "ctox_browser_automation",
            "phase": "cross-subject-replay",
            "input": {
                "owner_api_map_artifact": "authz/user-a/same-origin-api-map.json"
            },
            "harness_tool": {
                "kind": "freeform",
                "name": "ctox_browser_automation",
                "freeform_source": "return { count: globalThis.ctoxAuthzReplayCandidates.length };"
            },
            "ctox_cli": {
                "program": "ctox",
                "args": ["web", "browser-automation", "--session-id", "browser_session_user_b"],
                "stdin": "harness_tool.freeform_source"
            }
        });

        let source = browser_automation_source_from_stage_command(&state, &command).unwrap();
        assert!(source.starts_with("globalThis.ctoxAuthzReplayCandidates = "));
        assert!(source.contains("/api/projects/123"));
        assert!(source.contains("owner_body_hash"));
        assert!(!source.contains("/assets/app.8f3c1a2b.js"));
        assert!(!source.contains("/assets/project-123.css"));
        assert!(!source.contains("\"endpoint\":\"/api/projects\""));
        assert!(!source.contains("/pricing"));
        assert!(source.contains("return { count: globalThis.ctoxAuthzReplayCandidates.length };"));

        cleanup_test_dir(&root);
    }

    #[test]
    fn appsec_expected_authz_artifact_writer_redacts_browser_streams() {
        let root = make_fake_ctox_root("appsec-authz-redacted-artifact");
        let state = root.join("runtime/appsec/default");
        fs::create_dir_all(&state).unwrap();
        let command = serde_json::json!({
            "kind": "ctox-cli",
            "tool": "ctox_browser_automation",
            "phase": "cross-subject-replay",
            "target": "https://example.test",
            "actor_subject": "user-b",
            "owner_subject": "user-a",
            "expected_artifact": "authz/replay/user-a-as-user-b.json"
        });
        let output = serde_json::json!({
            "ok": true,
            "browser_stream": "rxdb",
            "screenshot": "redacted",
            "result": {
                "cases": [{
                    "actor_subject": "user-b",
                    "owner_subject": "user-a",
                    "endpoint": "/api/projects/123",
                    "method": "GET",
                    "expected": "deny",
                    "actual_status": 200,
                    "result": "failed"
                }],
                "browser_stream": "rxdb"
            }
        });

        let artifact = persist_appsec_command_expected_artifact(&state, &command, &output)
            .unwrap()
            .unwrap();
        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&artifact).unwrap()).unwrap();
        assert!(serde_json::to_string(&written)
            .unwrap()
            .contains("/api/projects/123"));
        assert!(!serde_json::to_string(&written)
            .unwrap()
            .contains("browser_stream"));
        assert!(written.get("screenshot").is_none());
        assert!(written.pointer("/result/screenshot").is_none());

        cleanup_test_dir(&root);
    }

    #[test]
    fn appsec_worker_blocks_browser_automation_with_unresolved_session_placeholder() {
        let root = make_fake_ctox_root("appsec-web-stack-browser-placeholder");
        let state = root.join("runtime/appsec/default");
        fs::create_dir_all(&state).unwrap();
        let stage = serde_json::json!({
            "run_commands": [{
                "kind": "ctox-cli",
                "tool": "ctox_browser_automation",
                "harness_tool": {
                    "kind": "freeform",
                    "name": "ctox_browser_automation",
                    "freeform_source": "const sessionIdRef = \"${session_id:user-a}\"; return { sessionIdRef };"
                },
                "ctox_cli": {
                    "program": "ctox",
                    "args": ["web", "browser-automation", "--timeout-ms", "1000"],
                    "stdin": "harness_tool.freeform_source"
                }
            }]
        });

        let execution = execute_appsec_stage_commands(&root, &state, &stage).unwrap();
        assert_eq!(
            execution
                .pointer("/commands/0/ok")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        assert_eq!(
            execution
                .pointer("/commands/0/status")
                .and_then(serde_json::Value::as_str),
            Some("blocked-placeholder-required")
        );

        cleanup_test_dir(&root);
    }

    fn make_fake_ctox_root(name: &str) -> PathBuf {
        let root = unique_test_dir(name);
        create_fake_ctox_root(&root);
        root
    }

    fn create_fake_ctox_root(root: &Path) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("contracts/history")).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"ctox\"\n").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(
            root.join("contracts/history/creation-ledger.md"),
            "# ledger\n",
        )
        .unwrap();
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ctox-main-tests-{name}-{unique}"))
    }

    fn cleanup_test_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }
}
