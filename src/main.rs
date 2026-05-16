use anyhow::Context;
use std::path::{Path, PathBuf};

mod api_costs;
mod autonomy;
mod capabilities;
mod communication;
mod context;
mod doc_stack;
mod execution;
mod export;
mod install;
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
  ctox stop                      stop the mission loop
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
  ctox business-os status        show bundled Business OS template/install state
  ctox business-os install --target <empty-dir> [--init-git]
                                 create a separate customer-owned Business OS repo

ENGINE / GPU
  ctox doctor                    health check — update available? hints

RUN / EXEC
  ctox runtime switch <model> <quality|performance> [--context 128k|256k] [--timeout <secs>]
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
  ctox scrape <subcmd>           scraping and extraction helpers
  ctox doc <subcmd>              document stack helpers
  ctox verification <subcmd>     verification records and evidence checks
  ctox skills <subcmd>           system/user skill catalog and pack management

GOVERNANCE / MISSION
  ctox service --foreground      run the daemon loop in the foreground
  ctox governance <subcmd>       governance decisions and audits
  ctox channel <subcmd>          communication channels (email, jami, webrtc)
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

fn main() -> anyhow::Result<()> {
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
            "upgrade" | "update" | "version" | "status" | "doctor" => return true,
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
                        &root, &audio_path
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
                        "usage: ctox runtime switch <model> <quality|performance> [--context 128k|256k] [--timeout <secs>]",
                    )?;
                let preset = args
                    .get(3)
                    .context(
                        "usage: ctox runtime switch <model> <quality|performance> [--context 128k|256k] [--timeout <secs>]",
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
                "usage: ctox runtime switch <model> <quality|performance> [--context 128k|256k] [--timeout <secs>] | ctox runtime embedding-doctor | ctox runtime embedding-smoke [--token-id <id>] | ctox runtime stt-doctor | ctox runtime stt-smoke <wav-path> | ctox runtime stt-realtime-smoke <wav-path> | ctox runtime tts-doctor | ctox runtime tts-smoke [--text <text>] | ctox runtime openrouter-tool-smoke [--model <id>] [--tool-choice auto|required|named|all]"
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
            if let Some(level_str) = find_flag_value(&args[1..], "--autonomy") {
                let level = autonomy::AutonomyLevel::from_str_lossy(level_str);
                std::env::set_var("CTOX_AUTONOMY_LEVEL", level.as_str());
            } else if flags.contains(&"--auto-approve-gates") {
                eprintln!(
                    "warning: --auto-approve-gates is deprecated; use --autonomy progressive"
                );
                std::env::set_var("CTOX_AUTONOMY_LEVEL", "progressive");
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
            println!("{}", service::stop_background(root)?);
            Ok(())
        }
        Some("status") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&service::service_status_snapshot(root)?)?
            );
            Ok(())
        }
        Some("work-hours") => service::working_hours::handle_work_hours_command(root, &args[1..]),
        Some("tui") => tui::run_tui(root),
        Some("business-os") | Some("business") => {
            service::business_os::handle_business_os_command(root, &args[1..])
        }
        Some("turn") => service::turn_ledger::handle_turn_command(root, &args[1..]),
        Some("harness-flow") => service::harness_flow::handle_harness_flow_command(root, &args[1..]),
        Some("process-mining") => {
            service::process_mining::handle_process_mining_command(root, &args[1..])
        }
        Some("harness-mining") => {
            service::harness_mining::handle_harness_mining_command(root, &args[1..])
        }
        Some("tui-smoke") => {
            let page = args.get(1).map(String::as_str).unwrap_or("chat");
            let width: u16 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(120);
            let height: u16 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(40);
            tui::run_tui_smoke(&root, page, width, height)
        }
        Some("browser") => browser::handle_browser_command(&root, &args[1..]),
        Some("channel") => channels::handle_channel_command(&root, &args[1..]),
        Some("doc") => doc::handle_doc_command(&root, &args[1..]),
        Some("follow-up") => follow_up::handle_follow_up_command(&args[1..]),
        Some("governance") => governance::handle_governance_command(&root, &args[1..]),
        Some("jami-daemon") => {
            communication::jami_native::handle_daemon_command(&root, &args[1..])
        }
        Some("knowledge") => service::run_knowledge_data(&root, &args[1..]),
        Some("meeting") => {
            communication::meeting_native::handle_meeting_command(&root, &args[1..])
        }
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
                    (Some(tail.join(" ")), 131_072_i64)
                }
            } else {
                (None, 131_072_i64)
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
        loop {
            let status = service::service_status_snapshot(root)?;
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
            std::thread::sleep(std::time::Duration::from_millis(250));
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
        .context("usage: ctox continuity-update --kind <narrative|anchors|focus> --mode <full|replace|diff> [--conversation-id <id>] [--db <path>] [--find <text>] [--replace <text>]")?;
    // conversation_id is optional: defaults to the constant CTOX chat
    // conversation id used by the service daemon. Codex-exec children
    // never need to know it explicitly.
    let conversation_id: i64 = match find_flag_value(args, "--conversation-id") {
        Some(v) => v.parse().context("failed to parse --conversation-id")?,
        None => inference::turn_loop::CHAT_CONVERSATION_ID,
    };
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
    candidate.join("Cargo.toml").is_file()
        && candidate.join("src/main.rs").is_file()
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

#[cfg(test)]
mod tests {
    use super::{
        chat_status_has_completed_since, find_ctox_root_from_ancestors, looks_like_ctox_root,
        openrouter_tool_smoke_summary, persist_runtime_turn_timeout, resolve_chat_attachment_paths,
        resolve_runtime_ctox_root, validated_workspace_root_override,
    };
    use crate::execution::models::runtime_env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

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
